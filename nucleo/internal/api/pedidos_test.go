//go:build unix

package api

import (
	"bytes"
	"encoding/json"
	"image"
	"image/png"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/navegador"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/sessoes"
)

const tokenTela = "token-da-tela"

// servidorComAgentes é o servidor com o canal de verdade na frente: o token
// da tela e os tokens dos agentes, e a pasta onde ficam as configurações MCP.
func servidorComAgentes(t *testing.T) (*httptest.Server, *Servidor) {
	t.Helper()
	dir := t.TempDir()
	banco, err := dados.Abrir(filepath.Join(dir, "dados"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { banco.Fechar() })
	servidor := &Servidor{Bytes: new(atomic.Int64), Banco: banco, DirDados: dir, Versao: "teste", Tempos: temposDeTeste,
		Fichas: canal.NovasFichas(), DirCanal: filepath.Join(dir, "run"), Executavel: "/bin/true"}
	os.MkdirAll(servidor.DirCanal, 0o700)
	srv := httptest.NewServer(canal.Autenticar(tokenTela, servidor.Fichas, servidor.Rotas()))
	t.Cleanup(func() { servidor.Agentes.FecharTodos(); servidor.Navegadores.FecharTodos(); servidor.Encerrar() })
	t.Cleanup(srv.Close)
	return srv, servidor
}

// pedirComo faz o pedido com o token dado e devolve o status e o corpo.
func pedirComo(t *testing.T, token, metodo, url string, corpo any) (int, map[string]any) {
	t.Helper()
	var leitor io.Reader = bytes.NewReader(nil)
	if corpo != nil {
		b, _ := json.Marshal(corpo)
		leitor = bytes.NewReader(b)
	}
	r, _ := http.NewRequest(metodo, url, leitor)
	r.Header.Set("Authorization", "Bearer "+token)
	resposta, err := http.DefaultClient.Do(r)
	if err != nil {
		t.Fatal(err)
	}
	defer resposta.Body.Close()
	bruto, _ := io.ReadAll(resposta.Body)
	var resultado map[string]any
	json.Unmarshal(bruto, &resultado)
	return resposta.StatusCode, resultado
}

// claudeQueAnota é um "claude" que grava os argumentos e cada linha que
// recebe. Com FALSO_APROVACAO, começa pedindo aprovação.
func claudeQueAnota(t *testing.T) (argumentos, linhas string) {
	t.Helper()
	bin := t.TempDir()
	argumentos, linhas = filepath.Join(bin, "args"), filepath.Join(bin, "linhas")
	script := `#!/bin/sh
echo "$@" > "` + argumentos + `"
if [ -n "$FALSO_APROVACAO" ]; then printf 'Do you want to proceed?\n\342\235\257 1. Yes\n'; read r; sleep 0.3; echo aprovado; fi
echo pronto
while read linha; do echo "$linha" >> "` + linhas + `"; sleep 0.2; echo "fazendo"; done
`
	if err := os.WriteFile(filepath.Join(bin, "claude"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", bin+":"+os.Getenv("PATH"))
	t.Setenv("CLAUDE_CONFIG_DIR", t.TempDir())
	return argumentos, linhas
}

// esperarAte repete a condição até ela valer (até 10 s): os testes esperam
// o agente parar e o pedido andar.
func esperarAte(t *testing.T, descricao string, ok func() bool) {
	t.Helper()
	limite := time.Now().Add(10 * time.Second)
	for !ok() {
		if time.Now().After(limite) {
			t.Fatalf("esperando %s", descricao)
		}
		time.Sleep(50 * time.Millisecond)
	}
}

func lerArquivo(caminho string) string {
	b, _ := os.ReadFile(caminho)
	return string(b)
}

func montarTarefas(t *testing.T, url string) (pasta string) {
	t.Helper()
	pasta = t.TempDir()
	pedirComo(t, tokenTela, "POST", url+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	pedirComo(t, tokenTela, "POST", url+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedirComo(t, tokenTela, "POST", url+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-web", "caminho": pasta})
	pedirComo(t, tokenTela, "POST", url+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Nova tela de pedidos"})
	pedirComo(t, tokenTela, "POST", url+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Relatório do cliente-x"})
	return pasta
}

func TestTokenDoAgenteSoValeParaATarefaDele(t *testing.T) {
	argumentos, _ := claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/2/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})

	// O Claude Code recebeu a configuração MCP; o token não vai na linha de comando.
	esperarAte(t, "os argumentos do claude", func() bool { return strings.Contains(lerArquivo(argumentos), "--mcp-config") })
	dir := filepath.Join(servidor.DirCanal, "agentes")
	args := lerArquivo(argumentos)
	token := lerArquivo(filepath.Join(dir, "1.token"))
	if !strings.Contains(args, filepath.Join(dir, "1.mcp.json")) && !strings.Contains(args, filepath.Join(dir, "2.mcp.json")) || !strings.Contains(args, "--allowedTools mcp__colmeia") {
		t.Errorf("argumentos: %s", args)
	}
	if len(token) != 64 || strings.Contains(args, token) {
		t.Fatalf("token: %d caracteres, nos argumentos: %v", len(token), strings.Contains(args, token))
	}
	for _, nome := range []string{"1.token", "1.mcp.json"} {
		if info, _ := os.Stat(filepath.Join(dir, nome)); info.Mode().Perm() != 0o600 {
			t.Errorf("%s com permissão %o", nome, info.Mode().Perm())
		}
	}
	if info, _ := os.Stat(dir); info.Mode().Perm() != 0o700 {
		t.Errorf("pasta dos agentes com permissão %o", info.Mode().Perm())
	}
	var config struct {
		Servidores map[string]struct {
			Tipo    string   `json:"type"`
			Comando string   `json:"command"`
			Args    []string `json:"args"`
		} `json:"mcpServers"`
	}
	json.Unmarshal([]byte(lerArquivo(filepath.Join(dir, "1.mcp.json"))), &config)
	if c := config.Servidores["colmeia"]; c.Tipo != "stdio" || c.Comando != "/bin/true" || c.Args[0] != "mcp" || strings.Contains(strings.Join(c.Args, " "), token) {
		t.Errorf("configuração MCP: %+v", config)
	}

	// O token do agente lê a própria tarefa...
	status, r := pedirComo(t, token, "GET", srv.URL+"/v1/agente/tarefa", nil)
	if tarefa, _ := r["tarefa"].(map[string]any); status != http.StatusOK || tarefa["titulo"] != "Nova tela de pedidos" {
		t.Fatalf("ler a tarefa: %d %v", status, r)
	}
	// ...e não entra em nenhuma rota da tela, nem na tarefa de outro.
	for _, rota := range []string{"/v1/tarefas/1/agentes", "/v1/tarefas/2/pedidos", "/v1/perfis", "/v1/anexos/1", "/v1/agentes/1/terminal"} {
		if status, _ := pedirComo(t, token, "GET", srv.URL+rota, nil); status != http.StatusUnauthorized {
			t.Errorf("token do agente em %s: %d", rota, status)
		}
	}
	// O token da tela não entra nas rotas dos agentes.
	if status, _ := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/agente/tarefa", nil); status != http.StatusUnauthorized {
		t.Errorf("token da tela na rota do agente: %d", status)
	}

	// Complementa a nota da daily de hoje; o pedido de outra tarefa é recusado.
	status, nota := pedirComo(t, token, "PUT", srv.URL+"/v1/agente/nota", map[string]any{"texto": "Total de testes: 42", "modo": "complementar"})
	if status != http.StatusOK || nota["texto"] != "Total de testes: 42" || nota["periodo"] != hoje() {
		t.Errorf("complementar: %d %v", status, nota)
	}
	status, _ = pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/2/pedidos", map[string]any{"texto": "traga os números", "tipo": "daily", "periodo": hoje()})
	if status != http.StatusOK {
		t.Fatalf("pedido na tarefa 2: %d", status)
	}
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/pedidos/1/concluir", map[string]any{}); status != http.StatusBadRequest {
		t.Errorf("concluir pedido de outra tarefa: %d %v", status, r)
	}
	if status, _ := pedirComo(t, token, "PUT", srv.URL+"/v1/agente/nota", map[string]any{"texto": "x", "modo": "complementar", "tarefa": 2}); status != http.StatusBadRequest {
		t.Errorf("campo desconhecido aceito: %d", status)
	}

	// Removido o agente, o token deixa de valer e os arquivos somem.
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/agentes/1", nil)
	if status, _ := pedirComo(t, token, "GET", srv.URL+"/v1/agente/tarefa", nil); status != http.StatusUnauthorized {
		t.Errorf("token de agente removido: %d", status)
	}
	if _, err := os.Stat(filepath.Join(dir, "1.token")); !os.IsNotExist(err) {
		t.Error("o arquivo do token ficou depois de remover o agente")
	}
}

func TestPedidoEntraSoQuandoOAgenteEspera(t *testing.T) {
	SemDigitar = 0
	t.Cleanup(func() { SemDigitar = 5 * time.Second })
	_, linhas := claudeQueAnota(t)
	t.Setenv("FALSO_APROVACAO", "1")
	srv, servidor := servidorComAgentes(t)
	pasta := montarTarefas(t, srv.URL)

	// Sem agente: o destino é abrir um Claude Code, retomando a última conversa.
	config := os.Getenv("CLAUDE_CONFIG_DIR")
	conversa := "81701644-dffb-4e29-9873-ea49ec35ec50"
	os.MkdirAll(sessoes.PastaDoProjeto(config, pasta), 0o700)
	os.WriteFile(filepath.Join(sessoes.PastaDoProjeto(config, pasta), conversa+".jsonl"), []byte(`{"type":"ai-title","aiTitle":"Tela de pedidos"}`+"\n"), 0o600)
	status, destino := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/pedidos/destino", nil)
	if status != http.StatusOK || destino["acao"] != "novo" || destino["conversa"] != "Tela de pedidos" {
		t.Fatalf("destino sem agente: %d %v", status, destino)
	}
	for _, corpo := range []map[string]any{
		{"texto": "senha: hunter2222", "tipo": "daily", "periodo": hoje()},
		{"texto": "x", "tipo": "daily", "periodo": "ontem"},
		{"texto": "", "tipo": "daily", "periodo": hoje()},
	} {
		if status, _ := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/pedidos", corpo); status != http.StatusBadRequest {
			t.Errorf("pedido %v aceito: %d", corpo, status)
		}
	}
	if agentes, _ := servidor.Banco.ListarAgentes(t.Context(), 1); len(agentes) != 0 {
		t.Fatalf("um pedido recusado abriu agente: %v", agentes)
	}

	status, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/pedidos", map[string]any{"texto": "Traga o total de testes", "tipo": "daily", "periodo": hoje()})
	if status != http.StatusOK {
		t.Fatalf("criar pedido: %d %v", status, r)
	}
	agentes, _ := servidor.Banco.ListarAgentes(t.Context(), 1)
	if len(agentes) != 1 || agentes[0].Ferramenta != "claude" || agentes[0].Sessao != conversa {
		t.Fatalf("agente aberto para o pedido: %+v", agentes)
	}
	// O agente começa pedindo aprovação: o pedido espera na fila.
	esperarAte(t, "o agente pedir aprovação", func() bool {
		s, ok := servidor.Agentes.Pegar(agentes[0].ID)
		if !ok {
			return false
		}
		_, motivo, _ := s.Estado()
		return motivo == "pede aprovação"
	})
	time.Sleep(300 * time.Millisecond)
	if p, _ := servidor.Banco.Pedido(t.Context(), 1); p.Estado != dados.PedidoFila {
		t.Fatalf("pedido entregue durante a aprovação: %+v", p)
	}
	// Você aprova no terminal; quando ele volta a esperar, o pedido entra.
	s, _ := servidor.Agentes.Pegar(agentes[0].ID)
	s.Escrever([]byte("1\r"))
	esperarAte(t, "o pedido ser entregue", func() bool {
		p, _ := servidor.Banco.Pedido(t.Context(), 1)
		return p.Estado == dados.PedidoEntregue
	})
	esperarAte(t, "o pedido chegar ao terminal", func() bool { return strings.Contains(lerArquivo(linhas), "pedido 1]") })
	recebido := lerArquivo(linhas)
	if !strings.Contains(recebido, "[Pedido da daily de "+time.Now().Format("02/01")+" · tarefa #1 · pedido 1] Traga o total de testes") ||
		!strings.Contains(recebido, "complementar_nota (tipo daily, período "+hoje()+", pedido 1)") || !strings.Contains(recebido, "Seja breve") {
		t.Errorf("texto no terminal: %q", recebido)
	}

	// O agente responde pela nota com o número do pedido: o pedido continua
	// aberto (ainda pode vir a captura) até concluir_pedido.
	token := lerArquivo(filepath.Join(servidor.DirCanal, "agentes", "1.token"))
	status, r = pedirComo(t, token, "GET", srv.URL+"/v1/agente/tarefa", nil)
	if pedidos, _ := r["pedidos"].([]any); status != http.StatusOK || len(pedidos) != 1 {
		t.Errorf("pedidos abertos na tarefa: %v", r)
	}
	if status, r := pedirComo(t, token, "PUT", srv.URL+"/v1/agente/nota", map[string]any{"texto": "42 testes", "modo": "complementar", "pedido": 1}); status != http.StatusOK {
		t.Fatalf("responder: %d %v", status, r)
	}
	if p, _ := servidor.Banco.Pedido(t.Context(), 1); p.Estado != dados.PedidoEntregue {
		t.Errorf("pedido fechado só pela nota: %+v", p)
	}
	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/pedidos/1/concluir", map[string]any{}); status != http.StatusOK || r["estado"] != dados.PedidoRespondido {
		t.Fatalf("concluir: %d %v", status, r)
	}

	// Um segundo pedido vai ao mesmo agente (ativo); cancelar só na fila.
	status, destino = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/pedidos/destino", nil)
	if destino["acao"] != "ativo" {
		t.Errorf("destino com agente ativo: %v", destino)
	}
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/pedidos", map[string]any{"texto": "Capture prints", "tipo": "daily", "periodo": hoje()})
	esperarAte(t, "o segundo pedido ser entregue", func() bool {
		p, _ := servidor.Banco.Pedido(t.Context(), 2)
		return p.Estado == dados.PedidoEntregue
	})
	if status, _ := pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/pedidos/2", nil); status != http.StatusBadRequest {
		t.Errorf("cancelar entregue: %d", status)
	}
	// Ele responde pela nota e esquece concluir_pedido: o pedido fecha quando
	// ele termina a vez (trabalha e volta a esperar você).
	esperarAte(t, "o agente terminar a vez", func() bool {
		_, motivo, _ := s.Estado()
		return motivo == "esperando resposta"
	})
	pedirComo(t, token, "PUT", srv.URL+"/v1/agente/nota", map[string]any{"texto": "Capturas anexadas", "modo": "complementar", "pedido": 2})
	if p, _ := servidor.Banco.Pedido(t.Context(), 2); p.Estado != dados.PedidoEntregue {
		t.Errorf("pedido 2 fechado só pela nota: %+v", p)
	}
	s.Escrever([]byte("mais uma volta\r"))
	esperarAte(t, "o pedido 2 fechar no fim da vez", func() bool {
		p, _ := servidor.Banco.Pedido(t.Context(), 2)
		return p.Estado == dados.PedidoRespondido
	})
	// Um terceiro, entregue, e o agente para: falhou, com o motivo.
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/pedidos", map[string]any{"texto": "Traga a cobertura", "tipo": "daily", "periodo": hoje()})
	esperarAte(t, "o terceiro pedido ser entregue", func() bool {
		p, _ := servidor.Banco.Pedido(t.Context(), 3)
		return p.Estado == dados.PedidoEntregue
	})
	pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/agentes/1", nil)
	if p, _ := servidor.Banco.Pedido(t.Context(), 3); p.Estado != dados.PedidoFalhou || p.Motivo == "" {
		t.Errorf("pedido de agente removido: %+v", p)
	}
	// A linha do tempo cita o pedido.
	resposta, _ := http.NewRequest("GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	resposta.Header.Set("Authorization", "Bearer "+tokenTela)
	res, err := http.DefaultClient.Do(resposta)
	if err != nil {
		t.Fatal(err)
	}
	bruto, _ := io.ReadAll(res.Body)
	res.Body.Close()
	for _, texto := range []string{"Pediu ao agente: «Traga o total de testes»", "O agente respondeu o pedido «Traga o total de testes»", "O agente complementou a nota da daily"} {
		if !strings.Contains(string(bruto), texto) {
			t.Errorf("a linha do tempo não tem %q", texto)
		}
	}
}

func TestNotaComVersaoDaTela(t *testing.T) {
	srv, _ := servidorComAgentes(t)
	montarTarefas(t, srv.URL)
	url := srv.URL + "/v1/tarefas/1/notas"
	vazia := ""
	status, n := pedirComo(t, tokenTela, "PUT", url, map[string]any{"tipo": "daily", "periodo": "2026-10-02", "texto": "Primeira", "versao": vazia})
	if status != http.StatusOK {
		t.Fatalf("primeira nota: %d %v", status, n)
	}
	status, r := pedirComo(t, tokenTela, "PUT", url, map[string]any{"tipo": "daily", "periodo": "2026-10-02", "texto": "Outra", "versao": vazia})
	if status != http.StatusConflict || r["texto"] != "Primeira" || r["versao"] != n["atualizada_em"] {
		t.Errorf("versão antiga: %d %v", status, r)
	}
	if status, _ := pedirComo(t, tokenTela, "PUT", url, map[string]any{"tipo": "daily", "periodo": "2026-10-02", "texto": "Outra", "versao": n["atualizada_em"]}); status != http.StatusOK {
		t.Errorf("versão certa: %d", status)
	}
	// Sem versão, como antes.
	if status, _ := pedirComo(t, tokenTela, "PUT", url, map[string]any{"tipo": "daily", "periodo": "2026-10-02", "texto": "Sem versão"}); status != http.StatusOK {
		t.Errorf("sem versão: %d", status)
	}
}

func TestArquivosDaTarefa(t *testing.T) {
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	pasta := montarTarefas(t, srv.URL)
	os.MkdirAll(filepath.Join(pasta, ".git"), 0o700)
	os.WriteFile(filepath.Join(pasta, ".git", "config"), []byte("[core]"), 0o600)
	os.WriteFile(filepath.Join(pasta, "README.md"), []byte("# loja-web\n"), 0o600)
	var b bytes.Buffer
	png.Encode(&b, image.NewGray(image.Rect(0, 0, 40, 20)))
	os.WriteFile(filepath.Join(pasta, "tela.png"), b.Bytes(), 0o600)

	status, r := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/arquivos", nil)
	if entradas, _ := r["entradas"].([]any); status != http.StatusOK || len(entradas) != 3 {
		t.Fatalf("listar: %d %v", status, r)
	}
	status, r = pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/arquivo?caminho=README.md", nil)
	if status != http.StatusOK || r["tipo"] != "texto" || r["caminho_absoluto"] != filepath.Join(pasta, "README.md") {
		t.Errorf("ver: %d %v", status, r)
	}
	for _, caminho := range []string{"../x", ".git/config", "/etc/passwd"} {
		if status, r := pedirComo(t, tokenTela, "GET", srv.URL+"/v1/tarefas/1/arquivo?caminho="+caminho, nil); status != http.StatusBadRequest {
			t.Errorf("%s: %d %v", caminho, status, r)
		}
	}
	req, _ := http.NewRequest("GET", srv.URL+"/v1/tarefas/1/arquivo/imagem?caminho=tela.png", nil)
	req.Header.Set("Authorization", "Bearer "+tokenTela)
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer res.Body.Close()
	if res.Header.Get("Content-Type") != "image/png" || res.StatusCode != http.StatusOK {
		t.Errorf("imagem: %d %s", res.StatusCode, res.Header.Get("Content-Type"))
	}

	// O agente anexa um PNG de dentro da pasta; de fora, não.
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(filepath.Join(servidor.DirCanal, "agentes", "1.token"))
	status, r = pedirComo(t, token, "POST", srv.URL+"/v1/agente/anexos", map[string]any{"caminho": "tela.png", "legenda": "Tela nova"})
	if status != http.StatusOK || r["largura"] != 40.0 {
		t.Errorf("anexar: %d %v", status, r)
	}
	if anexo, _ := servidor.Banco.Anexo(t.Context(), 1); anexo.TarefaID != 1 || anexo.Legenda != "Tela nova" || anexo.Nome != "tela.png" {
		t.Errorf("anexo: %+v", anexo)
	}
	for _, caminho := range []string{"../fora.png", "/etc/passwd", "README.md", ".git/config"} {
		if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/anexos", map[string]any{"caminho": caminho}); status != http.StatusBadRequest {
			t.Errorf("anexar %s: %d %v", caminho, status, r)
		}
	}
}

// Com COLMEIA_TESTE_NAVEGADOR=1 e um Chrome instalado: o agente abre um
// arquivo da pasta no navegador da tarefa, captura e a captura vira anexo.
func TestAgenteAbreECapturaONavegador(t *testing.T) {
	if os.Getenv("COLMEIA_TESTE_NAVEGADOR") != "1" {
		t.Skip("defina COLMEIA_TESTE_NAVEGADOR=1 para abrir um Chrome de verdade")
	}
	if _, _, err := navegador.Encontrar(); err != nil {
		t.Skip(err)
	}
	claudeQueAnota(t)
	srv, servidor := servidorComAgentes(t)
	servidor.Navegadores.Headless = os.Getenv("DISPLAY") == "" || os.Getenv("COLMEIA_TESTE_HEADLESS") == "1"
	pasta := montarTarefas(t, srv.URL)
	os.WriteFile(filepath.Join(pasta, "index.html"), []byte(`<body style="background:#fc0"><h1>loja-web</h1></body>`), 0o600)
	pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	token := lerArquivo(filepath.Join(servidor.DirCanal, "agentes", "1.token"))

	if status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/navegador", map[string]any{"url": "javascript:alert(1)"}); status != http.StatusBadRequest {
		t.Errorf("javascript: %d %v", status, r)
	}
	status, r := pedirComo(t, token, "POST", srv.URL+"/v1/agente/navegador", map[string]any{"url": "file://" + filepath.Join(pasta, "index.html")})
	if status != http.StatusOK || r["descricao"] != "arquivo index.html" {
		t.Fatalf("abrir: %d %v", status, r)
	}
	status, r = pedirComo(t, token, "POST", srv.URL+"/v1/agente/navegador/captura", map[string]any{})
	if status != http.StatusOK || r["anexo"] == nil || r["png"] == "" {
		t.Fatalf("capturar: %d %v", status, r)
	}
	anexo, _ := servidor.Banco.Anexo(t.Context(), int64(r["anexo"].(float64)))
	if anexo.Origem != "captura" || anexo.Legenda != "Navegador: arquivo index.html" || anexo.TarefaID != 1 {
		t.Errorf("anexo: %+v", anexo)
	}
	// O usuário também captura pela tela.
	if status, r := pedirComo(t, tokenTela, "POST", srv.URL+"/v1/tarefas/1/navegador/captura", nil); status != http.StatusOK {
		t.Errorf("captura pela tela: %d %v", status, r)
	}
	if status, _ := pedirComo(t, tokenTela, "DELETE", srv.URL+"/v1/tarefas/1/navegador", nil); status != http.StatusOK {
		t.Errorf("fechar: %d", status)
	}
}
