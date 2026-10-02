//go:build unix

package api

import (
	"bytes"
	"context"
	"encoding/json"
	"image"
	"image/png"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/coder/websocket"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
)

// ouvinte lê o WebSocket de eventos de um perfil.
type ouvinte struct {
	t    *testing.T
	conn *websocket.Conn
	ctx  context.Context
}

func ouvir(t *testing.T, url string, perfil string) *ouvinte {
	t.Helper()
	ctx, cancelar := context.WithTimeout(context.Background(), 15*time.Second)
	t.Cleanup(cancelar)
	conn, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(url, "http")+"/v1/perfis/"+perfil+"/eventos", nil)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { conn.CloseNow() })
	o := &ouvinte{t, conn, ctx}
	if m := o.proxima(); m["tipo"] != "ola" {
		t.Fatalf("primeira mensagem: %v", m)
	}
	return o
}

func (o *ouvinte) proxima() map[string]any {
	o.t.Helper()
	_, bruto, err := o.conn.Read(o.ctx)
	if err != nil {
		o.t.Fatalf("lendo eventos: %v", err)
	}
	var m map[string]any
	json.Unmarshal(bruto, &m)
	return m
}

// esperar lê até chegar uma mensagem que satisfaça `ok`.
func (o *ouvinte) esperar(descricao string, ok func(map[string]any) bool) map[string]any {
	o.t.Helper()
	for {
		_, bruto, err := o.conn.Read(o.ctx)
		if err != nil {
			o.t.Fatalf("esperando %s: %v", descricao, err)
		}
		var m map[string]any
		json.Unmarshal(bruto, &m)
		if ok(m) {
			return m
		}
	}
}

func coluna(tipo, col, origem string) func(map[string]any) bool {
	return func(m map[string]any) bool {
		t, _ := m["tarefa"].(map[string]any)
		return m["tipo"] == tipo && t["coluna"] == col && (origem == "" || m["origem"] == origem)
	}
}

func estado(e string) func(map[string]any) bool {
	return func(m map[string]any) bool { return m["tipo"] == "agente.estado" && m["estado"] == e }
}

func TestEventosChegamSoAoPerfilCerto(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	um := ouvir(t, srv.URL, "1")
	dois := ouvir(t, srv.URL, "2")
	// O que a tela manda por aqui é ignorado, e a conexão segue.
	um.conn.Write(um.ctx, websocket.MessageText, []byte(`{"tipo":"tarefa.removida","tarefa_id":1}`))

	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})
	if m := um.proxima(); m["tipo"] != "projeto.criado" {
		t.Errorf("primeiro evento do perfil 1: %v", m)
	}
	m := um.proxima()
	if tarefa, _ := m["tarefa"].(map[string]any); m["tipo"] != "tarefa.criada" || tarefa["titulo"] != "Analisar" || m["seq"] == nil {
		t.Errorf("tarefa criada: %v", m)
	}
	// O perfil 2 só recebe o que é dele.
	pedir(t, "POST", srv.URL+"/v1/perfis/2/workspaces", map[string]string{"nome": "Casa"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/2/projetos", map[string]string{"nome": "notas", "caminho": t.TempDir()})
	if m := dois.proxima(); m["tipo"] != "projeto.criado" {
		t.Errorf("o perfil 2 recebeu %v", m)
	}

	// O retrato do quadro traz o seq: o que veio antes dele já está dentro.
	resposta, _ := http.Get(srv.URL + "/v1/perfis/1/quadro")
	var quadro struct {
		Seq      float64
		Projetos []map[string]any
		Tarefas  []map[string]any
		Agentes  []map[string]any
	}
	json.NewDecoder(resposta.Body).Decode(&quadro)
	resposta.Body.Close()
	if len(quadro.Projetos) != 1 || len(quadro.Tarefas) != 1 || quadro.Seq < m["seq"].(float64) {
		t.Errorf("quadro: %+v", quadro)
	}
}

func TestEventosExigemTokenEOrigemLocal(t *testing.T) {
	banco := servidorComDados(t)
	pedir(t, "POST", banco.URL+"/v1/perfis", map[string]string{"nome": "P"})
	srv := httptest.NewServer(canal.ExigirToken("segredo", http.NotFoundHandler()))
	t.Cleanup(srv.Close)
	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	if _, resposta, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http")+"/v1/perfis/1/eventos", nil); err == nil || resposta.StatusCode != http.StatusUnauthorized {
		t.Errorf("sem token: %v", err)
	}
	// Uma página de outro site não abre o WebSocket, mesmo com o token.
	cabecalho := http.Header{"Origin": {"https://site-qualquer.example"}}
	_, resposta, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(banco.URL, "http")+"/v1/perfis/1/eventos", &websocket.DialOptions{HTTPHeader: cabecalho})
	if err == nil || resposta.StatusCode != http.StatusForbidden {
		t.Errorf("Origin de fora: %v", err)
	}
}

// claudeFalso põe no PATH um "claude" que responde a cada linha depois de um tempo.
func claudeFalso(t *testing.T) {
	t.Helper()
	bin := t.TempDir()
	script := "#!/bin/sh\necho pronto\nwhile read linha; do sleep 0.3; echo \"fazendo $linha\"; done\n"
	if err := os.WriteFile(filepath.Join(bin, "claude"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", bin+":"+os.Getenv("PATH"))
	t.Setenv("CLAUDE_CONFIG_DIR", t.TempDir())
}

func TestCartaoAndaSozinhoComOAgente(t *testing.T) {
	claudeFalso(t)
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})
	eventos := ouvir(t, srv.URL, "1")

	status, agente := pedir(t, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "claude", "papel": "dev"})
	if status != http.StatusOK || agente["estado"] != "trabalhando" {
		t.Fatalf("criar agente: %d %v", status, agente)
	}
	eventos.esperar("agente.iniciou", func(m map[string]any) bool { return m["tipo"] == "agente.iniciou" })
	// Começou num cartão do Backlog: a tarefa vai para Trabalhando sozinha.
	eventos.esperar("tarefa em trabalhando", coluna("tarefa.atualizada", "trabalhando", "automatico"))
	// Em silêncio, o Claude Code espera você: o cartão vai para Aguardando.
	m := eventos.esperar("agente aguardando", estado("aguardando"))
	if m["motivo"] != "esperando resposta" || m["desde_hora"] == "" {
		t.Errorf("estado aguardando: %v", m)
	}
	eventos.esperar("tarefa aguardando", coluna("tarefa.atualizada", "aguardando", "automatico"))

	// Você responde; ele volta a trabalhar e o cartão volta junto.
	ctx, cancelar := context.WithTimeout(context.Background(), 10*time.Second)
	defer cancelar()
	terminal, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http")+"/v1/agentes/1/terminal", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer terminal.CloseNow()
	terminal.Write(ctx, websocket.MessageBinary, []byte("isso\r"))
	eventos.esperar("agente trabalhando", estado("trabalhando"))
	eventos.esperar("tarefa de volta", coluna("tarefa.atualizada", "trabalhando", "automatico"))

	// Você move o cartão: o núcleo não desfaz.
	pedir(t, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "revisao"})
	// O agente pode voltar a esperar antes ou depois de a mudança chegar.
	suaMudanca, aguardando := coluna("tarefa.atualizada", "revisao", "voce"), estado("aguardando")
	viu := [2]bool{}
	eventos.esperar("sua mudança e o agente aguardando de novo", func(m map[string]any) bool {
		viu[0] = viu[0] || suaMudanca(m)
		viu[1] = viu[1] || aguardando(m)
		return viu[0] && viu[1]
	})
	terminal.Write(ctx, websocket.MessageBinary, []byte("mais\r"))
	eventos.esperar("agente trabalhando de novo", estado("trabalhando"))
	if _, lista := pedirLista(t, srv.URL+"/v1/projetos/1/tarefas"); lista[0]["coluna"] != "revisao" || lista[0]["coluna_auto"] != false {
		t.Errorf("o núcleo mexeu num cartão que você moveu: %v", lista[0])
	}

	// Removido pela Colmeia: terminou, sem erro.
	pedir(t, "DELETE", srv.URL+"/v1/agentes/1", nil)
	m = eventos.esperar("agente.terminou", func(m map[string]any) bool { return m["tipo"] == "agente.terminou" })
	if fim, _ := m["fim"].(map[string]any); fim["motivo"] != "removido" || fim["erro"] != false {
		t.Errorf("fim do agente removido: %v", m)
	}
}

func TestAgenteQueFalhaApareceComErro(t *testing.T) {
	bin := t.TempDir()
	os.WriteFile(filepath.Join(bin, "codex"), []byte("#!/bin/sh\nexit 3\n"), 0o700)
	t.Setenv("PATH", bin+":"+os.Getenv("PATH"))
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})
	eventos := ouvir(t, srv.URL, "1")
	pedir(t, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "codex", "papel": "dev"})
	m := eventos.esperar("agente.terminou", func(m map[string]any) bool { return m["tipo"] == "agente.terminou" })
	fim, _ := m["fim"].(map[string]any)
	if fim["erro"] != true || fim["codigo"] != 3.0 || fim["texto"] != "Codex (dev) parou com erro (código 3)" {
		t.Errorf("fim com erro: %v", m)
	}
	// Ao reabrir a tela, o erro vem no retrato do quadro.
	resposta, _ := http.Get(srv.URL + "/v1/perfis/1/quadro")
	var quadro struct{ Agentes []map[string]any }
	json.NewDecoder(resposta.Body).Decode(&quadro)
	resposta.Body.Close()
	if fim, _ := quadro.Agentes[0]["ultimo_fim"].(map[string]any); fim["erro"] != true {
		t.Errorf("quadro sem o último fim: %v", quadro.Agentes)
	}
}

func pngPequeno(t *testing.T) []byte {
	var b bytes.Buffer
	png.Encode(&b, image.NewRGBA(image.Rect(0, 0, 4, 3)))
	return b.Bytes()
}

func TestAnexos(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})
	enviar := func(url, tipo string, corpo []byte) (int, map[string]any) {
		r, _ := http.NewRequest("POST", url, bytes.NewReader(corpo))
		r.Header.Set("Content-Type", tipo)
		resposta, err := http.DefaultClient.Do(r)
		if err != nil {
			t.Fatal(err)
		}
		defer resposta.Body.Close()
		var m map[string]any
		json.NewDecoder(resposta.Body).Decode(&m)
		return resposta.StatusCode, m
	}
	if status, _ := enviar(srv.URL+"/v1/tarefas/1/anexos?origem=captura", "text/plain", pngPequeno(t)); status != http.StatusBadRequest {
		t.Errorf("sem image/png: %d", status)
	}
	if status, _ := enviar(srv.URL+"/v1/tarefas/1/anexos?origem=inventada", "image/png", pngPequeno(t)); status != http.StatusBadRequest {
		t.Errorf("origem inventada: %d", status)
	}
	if status, _ := enviar(srv.URL+"/v1/tarefas/1/anexos?origem=captura", "image/png", []byte("não é png")); status != http.StatusBadRequest {
		t.Errorf("corpo que não é PNG: %d", status)
	}
	status, anexo := enviar(srv.URL+"/v1/tarefas/1/anexos?origem=captura&legenda=tela", "image/png", pngPequeno(t))
	if status != http.StatusOK || anexo["largura"] != 4.0 {
		t.Fatalf("anexar: %d %v", status, anexo)
	}
	resposta, _ := http.Get(srv.URL + "/v1/anexos/1")
	conteudo, _ := io.ReadAll(resposta.Body)
	resposta.Body.Close()
	if resposta.Header.Get("Content-Type") != "image/png" || resposta.Header.Get("X-Content-Type-Options") != "nosniff" || !bytes.HasPrefix(conteudo, []byte("\x89PNG")) {
		t.Errorf("ler anexo: %v", resposta.Header)
	}
	if status, _ := pedir(t, "DELETE", srv.URL+"/v1/anexos/1", nil); status != http.StatusOK {
		t.Errorf("remover: %d", status)
	}
	if _, err := os.Stat(anexo["caminho"].(string)); !os.IsNotExist(err) {
		t.Error("o arquivo do anexo removido ficou no disco")
	}
	if resposta, _ := http.Get(srv.URL + "/v1/anexos/1"); resposta.StatusCode != http.StatusNotFound {
		t.Errorf("anexo removido: %d", resposta.StatusCode)
	}
}

func TestEncerrarPelaApi(t *testing.T) {
	encerrou := make(chan struct{})
	srv := httptest.NewServer((&Servidor{Versao: "teste", AoEncerrar: func() { close(encerrou) }}).Rotas())
	t.Cleanup(srv.Close)
	if status, _ := pedir(t, "POST", srv.URL+"/v1/encerrar", nil); status != http.StatusAccepted {
		t.Errorf("encerrar: %d", status)
	}
	select {
	case <-encerrou:
	case <-time.After(2 * time.Second):
		t.Error("o núcleo não foi encerrado")
	}
}

func TestLinhaDoTempoEResumos(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})
	pedir(t, "PATCH", srv.URL+"/v1/tarefas/1", map[string]string{"coluna": "concluido"})

	status, linha := pedir(t, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo", nil)
	dias, _ := linha["dias"].([]any)
	if status != http.StatusOK || len(dias) != 1 {
		t.Fatalf("linha do tempo: %d %v", status, linha)
	}
	itens := dias[0].(map[string]any)["itens"].([]any)
	if primeiro := itens[0].(map[string]any); primeiro["texto"] != "Concluiu “Analisar”." || primeiro["tipo"] != "concluiu" {
		t.Errorf("item mais novo: %v", primeiro)
	}
	// Paginação: uma página de 1 aponta para a próxima.
	if _, pagina := pedir(t, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo?limite=1", nil); pagina["proximo"] == 0.0 {
		t.Errorf("página de 1 sem próxima: %v", pagina)
	}
	hoje := time.Now().Format("2006-01-02")
	for _, ruim := range []string{
		"/v1/perfis/1/linha-do-tempo?limite=0",
		"/v1/perfis/1/linha-do-tempo?limite=501",
		"/v1/perfis/1/linha-do-tempo?de=01/09/2026&ate=" + hoje,
		"/v1/perfis/1/resumo?tipo=sprint&de=2026-01-01&ate=2026-06-30",
		"/v1/perfis/1/resumo?tipo=sprint&de=2026-09-10&ate=2026-09-01",
		"/v1/perfis/1/resumo?tipo=semanal",
	} {
		if status, _ := pedir(t, "GET", srv.URL+ruim, nil); status != http.StatusBadRequest {
			t.Errorf("%s: status %d, esperado 400", ruim, status)
		}
	}
	if status, _ := pedir(t, "GET", srv.URL+"/v1/perfis/1/linha-do-tempo?projeto=99", nil); status != http.StatusNotFound {
		t.Errorf("projeto de fora: %d", status)
	}
	status, daily := pedir(t, "GET", srv.URL+"/v1/perfis/1/resumo?tipo=daily", nil)
	if status != http.StatusOK || daily["texto"] != "Hoje: já concluí Analisar." {
		t.Errorf("daily: %d %v", status, daily)
	}
	if status, sprint := pedir(t, "GET", srv.URL+"/v1/perfis/1/resumo?tipo=sprint&ultimos=7", nil); status != http.StatusOK || sprint["ate"] != time.Now().Format("02/01/2006") {
		t.Errorf("sprint dos últimos 7 dias: %d %v", status, sprint["ate"])
	}
	if status, _ := pedir(t, "GET", srv.URL+"/v1/perfis/1/resumo?tipo=sprint&ultimos=93", nil); status != http.StatusBadRequest {
		t.Errorf("93 dias: %d", status)
	}
	resposta, _ := http.Get(srv.URL + "/v1/perfis/1/resumo?tipo=sprint&de=" + hoje + "&ate=" + hoje + "&formato=markdown")
	md, _ := io.ReadAll(resposta.Body)
	resposta.Body.Close()
	if resposta.Header.Get("Content-Type") != "text/markdown; charset=utf-8" || !strings.Contains(string(md), "- Analisar (") {
		t.Errorf("sprint em markdown: %s", md)
	}
}

func TestTerminalComumNaoTiraDoBacklog(t *testing.T) {
	srv := servidorComDados(t)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "W"})
	pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "clientes", "caminho": t.TempDir()})
	pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]string{"titulo": "Analisar"})
	eventos := ouvir(t, srv.URL, "1")
	if status, _ := pedir(t, "POST", srv.URL+"/v1/tarefas/1/agentes", map[string]string{"ferramenta": "shell", "papel": "dev"}); status != http.StatusOK {
		t.Fatalf("criar terminal: %d", status)
	}
	eventos.esperar("agente.iniciou", func(m map[string]any) bool { return m["tipo"] == "agente.iniciou" })
	if _, lista := pedirLista(t, srv.URL+"/v1/projetos/1/tarefas"); lista[0]["coluna"] != "backlog" {
		t.Errorf("um terminal aberto tirou a tarefa do Backlog: %v", lista[0])
	}
}
