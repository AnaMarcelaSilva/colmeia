//go:build unix

package api

import (
	"crypto/sha256"
	"encoding/hex"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/voz"
)

// servidorComVoz serve `servido` como modelo de voz num https de teste; o
// esperado é sempre `modeloDeTeste`.
func servidorComVoz(t *testing.T, servido []byte) *httptest.Server {
	t.Helper()
	fonte := httptest.NewTLSServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { w.Write(servido) }))
	t.Cleanup(fonte.Close)
	dir := t.TempDir()
	banco, err := dados.Abrir(filepath.Join(dir, "dados"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { banco.Fechar() })
	soma := sha256.Sum256(modeloDeTeste)
	g := voz.Novo(dir, voz.Modelo{Nome: "ggml-teste.bin", URL: fonte.URL + "/m.bin", SHA256: hex.EncodeToString(soma[:]), Tamanho: int64(len(modeloDeTeste))})
	g.Cliente = fonte.Client()
	servidor := &Servidor{Bytes: new(atomic.Int64), Banco: banco, DirDados: dir, Versao: "teste", Tempos: temposDeTeste, Voz: g}
	srv := httptest.NewServer(servidor.Rotas())
	t.Cleanup(func() { servidor.Encerrar() })
	t.Cleanup(srv.Close)
	return srv
}

var modeloDeTeste = []byte(strings.Repeat("whisper ", 20000))

func ditadoDoPerfil(t *testing.T, url string, id float64) bool {
	t.Helper()
	_, perfis := pedirLista(t, url+"/v1/perfis")
	for _, p := range perfis {
		if p["id"] == id {
			return p["ditado"] == true
		}
	}
	t.Fatalf("perfil %v sumiu", id)
	return false
}

func TestDitadoLigaSoComOModeloBaixado(t *testing.T) {
	srv := servidorComVoz(t, modeloDeTeste)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	if ditadoDoPerfil(t, srv.URL, 1) {
		t.Fatal("o ditado nasce desligado")
	}
	if _, e := pedir(t, "GET", srv.URL+"/v1/voz", nil); e["estado"] != "ausente" || e["tamanho"] != float64(len(modeloDeTeste)) {
		t.Fatalf("estado antes: %v", e)
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"ditado": true}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "Baixe o modelo") {
		t.Fatalf("ligou sem o modelo: %d %v", status, r)
	}

	eventos := ouvir(t, srv.URL, "1")
	if status, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", nil); status != http.StatusOK {
		t.Fatalf("baixar: %d %v", status, r)
	}
	eventos.esperar("voz.modelo pronto", func(m map[string]any) bool { return m["tipo"] == "voz.modelo" && m["estado"] == "pronto" })
	if m := eventos.esperar("perfil.ditado", func(m map[string]any) bool { return m["tipo"] == "perfil.ditado" }); m["ligado"] != true || m["perfil_id"] != 1.0 {
		t.Errorf("evento do ditado: %v", m)
	}
	if !ditadoDoPerfil(t, srv.URL, 1) || ditadoDoPerfil(t, srv.URL, 2) {
		t.Error("o download liga o ditado só de quem pediu")
	}
	_, e := pedir(t, "GET", srv.URL+"/v1/voz", nil)
	if e["estado"] != "pronto" || !strings.HasSuffix(e["caminho"].(string), filepath.Join("voz", "ggml-teste.bin")) {
		t.Fatalf("estado depois: %v", e)
	}
	// Com o modelo no disco, o outro perfil liga e desliga direto.
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/2", map[string]any{"ditado": true}); status != http.StatusOK {
		t.Errorf("ligar com o modelo: %d", status)
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"ditado": "sim"}); status != http.StatusBadRequest {
		t.Errorf("ditado que não é booleano: %d", status)
	}

	// Apagar o modelo desliga o ditado de todos.
	if status, r := pedir(t, "DELETE", srv.URL+"/v1/voz/modelo", nil); status != http.StatusOK || r["estado"] != "ausente" {
		t.Fatalf("apagar: %d %v", status, r)
	}
	if ditadoDoPerfil(t, srv.URL, 1) || ditadoDoPerfil(t, srv.URL, 2) {
		t.Error("o ditado continuou ligado sem o modelo")
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/99/voz/baixar", nil); status != http.StatusNotFound {
		t.Errorf("perfil que não existe: %d", status)
	}
	if _, r := pedir(t, "POST", srv.URL+"/v1/voz/cancelar", nil); r["cancelado"] != false {
		t.Errorf("cancelar sem download: %v", r)
	}
}

func TestModeloAdulteradoNaoLigaODitado(t *testing.T) {
	adulterado := append([]byte{}, modeloDeTeste...)
	adulterado[0] = 'W'
	srv := servidorComVoz(t, adulterado)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	eventos := ouvir(t, srv.URL, "1")
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", nil); status != http.StatusOK {
		t.Fatalf("baixar: %d", status)
	}
	m := eventos.esperar("voz.modelo erro", func(m map[string]any) bool { return m["tipo"] == "voz.modelo" && m["estado"] == "erro" })
	if !strings.Contains(m["erro"].(string), "SHA-256") {
		t.Errorf("erro do download: %v", m)
	}
	if ditadoDoPerfil(t, srv.URL, 1) {
		t.Error("ligou o ditado com um modelo recusado")
	}
	if _, e := pedir(t, "GET", srv.URL+"/v1/voz", nil); e["estado"] != "erro" || e["caminho"] != nil {
		t.Errorf("estado: %v", e)
	}
}

// perfilDe lê um perfil pela lista.
func perfilDe(t *testing.T, url string, id float64) map[string]any {
	t.Helper()
	_, perfis := pedirLista(t, url+"/v1/perfis")
	for _, p := range perfis {
		if p["id"] == id {
			return p
		}
	}
	t.Fatalf("perfil %v sumiu", id)
	return nil
}

func falar(t *testing.T, url, frase string) map[string]any {
	t.Helper()
	status, r := pedir(t, "POST", url+"/v1/perfis/1/voz/comando", map[string]string{"texto": frase})
	if status != http.StatusOK {
		t.Fatalf("%q: %d %v", frase, status, r)
	}
	return r
}

func TestComandosFalados(t *testing.T) {
	srv := servidorComVoz(t, modeloDeTeste)
	pasta := t.TempDir()
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Pessoal"})
	pedir(t, "POST", srv.URL+"/v1/perfis/1/workspaces", map[string]string{"nome": "trabalho"})
	if status, p := pedir(t, "POST", srv.URL+"/v1/workspaces/1/projetos", map[string]string{"nome": "loja-web", "caminho": pasta}); status != http.StatusOK {
		t.Fatalf("projeto: %d %v", status, p)
	}
	pedir(t, "POST", srv.URL+"/v1/projetos/1/comandos", map[string]any{"nome": "servidor", "comando": "sleep 30"})
	if status, r := pedir(t, "POST", srv.URL+"/v1/projetos/1/tarefas", map[string]any{"titulo": "Login com Google"}); status != http.StatusOK {
		t.Fatalf("tarefa: %d %v", status, r)
	}

	// Desligado, nada é entendido; ligar pede o modelo.
	if status, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/comando", map[string]string{"texto": "rodar a loja web"}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "desligados") {
		t.Fatalf("desligado: %d %v", status, r)
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_comandos": true}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "Baixe") {
		t.Fatalf("ligou sem o modelo: %d %v", status, r)
	}
	eventos := ouvir(t, srv.URL, "1")
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", map[string]string{"ligar": "comandos"}); status != http.StatusOK {
		t.Fatal(status)
	}
	if m := eventos.esperar("perfil.voz", tipo("perfil.voz")); m["voz"].(map[string]any)["voz_comandos"] != true {
		t.Fatalf("evento: %v", m)
	}
	if p := perfilDe(t, srv.URL, 1); p["voz_comandos"] != true || p["ditado"] != false {
		t.Fatalf("o download liga só os comandos: %v", p)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", map[string]string{"ligar": "abelha"}); status != http.StatusBadRequest {
		t.Errorf("o Whisper não liga a voz da abelha: %d", status)
	}

	// Rodar: uma configuração só, nome claro, roda na hora.
	if r := falar(t, srv.URL, "Colmeia, rodar a loja web."); r["texto"] != "Rodando o projeto loja-web." || r["confirmar"] != nil {
		t.Fatalf("rodar: %v", r)
	}
	eventos.esperar("rodou", tipo("comando.iniciou"))
	// De novo, com ela rodando: pergunta, e o Sim clicado roda.
	r := falar(t, srv.URL, "rodar a loja web")
	ficha, _ := r["confirmar"].(string)
	if ficha == "" || !strings.Contains(r["texto"].(string), "já está rodando") {
		t.Fatalf("rodar de novo: %v", r)
	}
	if status, r := pedir(t, "POST", srv.URL+"/v1/perfis/2/voz/confirmar", map[string]any{"ficha": ficha, "sim": true}); status != http.StatusOK || r["texto"] == "Rodando o projeto loja-web." {
		t.Errorf("a ficha valeu em outro perfil: %v", r)
	}
	if _, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/confirmar", map[string]any{"ficha": ficha, "sim": true}); r["texto"] != "Rodando o projeto loja-web." {
		t.Fatalf("confirmar: %v", r)
	}
	if _, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/confirmar", map[string]any{"ficha": ficha, "sim": true}); !strings.Contains(r["texto"].(string), "já não vale") {
		t.Errorf("a ficha valeu duas vezes: %v", r)
	}

	// Parar sempre pergunta; o "sim" falado confirma.
	if r := falar(t, srv.URL, "parar a loja web"); r["confirmar"] == nil || r["texto"] != "Parar o projeto loja-web?" {
		t.Fatalf("parar: %v", r)
	}
	if r := falar(t, srv.URL, "Sim."); r["texto"] != "Parei o projeto loja-web." {
		t.Fatalf("sim: %v", r)
	}
	if m := eventos.esperar("parou", tipo("comando.terminou")); m["parada"] != true {
		t.Errorf("fim: %v", m)
	}

	// Encerrar: o "não" desfaz a pergunta; com o "sim", a tarefa vai para Concluído.
	if r := falar(t, srv.URL, "encerrar a tarefa login com google"); r["confirmar"] == nil {
		t.Fatalf("encerrar: %v", r)
	}
	if r := falar(t, srv.URL, "não"); r["texto"] != "Tudo bem, não fiz nada." {
		t.Errorf("não: %v", r)
	}
	if r := falar(t, srv.URL, "sim"); !strings.Contains(r["texto"].(string), "nada esperando") {
		t.Errorf("sim sem pergunta: %v", r)
	}
	ficha = falar(t, srv.URL, "encerra a tarefa do login")["confirmar"].(string)
	if _, r := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/confirmar", map[string]any{"ficha": ficha, "sim": true}); !strings.Contains(r["texto"].(string), "foi para Concluído") {
		t.Fatalf("encerrar confirmado: %v", r)
	}
	if m := eventos.esperar("tarefa", tipo("tarefa.atualizada")); m["tarefa"].(map[string]any)["coluna"] != "concluido" {
		t.Errorf("tarefa: %v", m)
	}

	// Abrir devolve a tarefa para a tela abrir; perguntas não mexem em nada.
	if r := falar(t, srv.URL, "abrir a tarefa login com google"); r["abrir"] != 1.0 || r["confirmar"] != nil {
		t.Errorf("abrir: %v", r)
	}
	if r := falar(t, srv.URL, "como está a tarefa login com google?"); !strings.Contains(r["texto"].(string), "em Concluído") {
		t.Errorf("como está: %v", r)
	}
	if r := falar(t, srv.URL, "o que espera por mim?"); r["texto"] != "Nada espera você agora." {
		t.Errorf("o que espera: %v", r)
	}
	if r := falar(t, srv.URL, "qual a previsão do tempo"); !strings.HasPrefix(r["texto"].(string), "Não entendi.") {
		t.Errorf("ajuda: %v", r)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/comando", map[string]string{"texto": strings.Repeat("rodar ", 100)}); status != http.StatusBadRequest {
		t.Errorf("frase enorme: %d", status)
	}
	if status, _ := pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/comando", map[string]any{"texto": "sim", "acao": "rodar"}); status != http.StatusBadRequest {
		t.Errorf("campo a mais: %d", status)
	}
}

func TestOpcoesDaVozDaAbelha(t *testing.T) {
	srv := servidorComVoz(t, modeloDeTeste)
	pedir(t, "POST", srv.URL+"/v1/perfis", map[string]string{"nome": "Profissional"})
	eventos := ouvir(t, srv.URL, "1")
	pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/baixar", map[string]string{"ligar": "comandos"})
	eventos.esperar("perfil.voz", tipo("perfil.voz"))

	// A voz da abelha pede o Kokoro baixado.
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_abelha": true}); status != http.StatusBadRequest {
		t.Errorf("voz sem o Kokoro: %d %v", status, r)
	}
	if _, e := pedir(t, "GET", srv.URL+"/v1/voz/fala", nil); e["estado"] != "ausente" && e["estado"] != "indisponivel" {
		t.Errorf("estado da fala: %v", e)
	}

	// Ouvir o nome: precisa da abelha fixa; com o nome padrão, liga no mesmo pedido.
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_nome": true}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "abelha fixa") {
		t.Errorf("sem a abelha fixa: %d %v", status, r)
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"abelha_fixa": true, "voz_nome": true, "voz_sensibilidade": 7}); status != http.StatusOK {
		t.Fatalf("ligar: %d %v", status, r)
	}
	if p := perfilDe(t, srv.URL, 1); p["voz_nome"] != true || p["voz_sensibilidade"] != 7.0 {
		t.Errorf("%v", p)
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_sensibilidade": 12}); status != http.StatusBadRequest {
		t.Errorf("sensibilidade 12: %d", status)
	}

	// Outro nome: sem gravações não liga; o arquivo feito para ele vale.
	pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_nome": false, "mascote_nome": "Zuzu"})
	_, n := pedir(t, "GET", srv.URL+"/v1/perfis/1/voz/nome", nil)
	if n["padrao"] != false || n["gravado"] != false || !strings.HasSuffix(n["arquivo"].(string), filepath.Join("voz", "nome-1.json")) {
		t.Fatalf("nome: %v", n)
	}
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_nome": true}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "Grave o nome Zuzu") {
		t.Errorf("sem gravação: %d %v", status, r)
	}
	if err := os.WriteFile(n["arquivo"].(string), []byte(`{"nome":"zuzu","referencias":[]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, n := pedir(t, "GET", srv.URL+"/v1/perfis/1/voz/nome", nil); n["gravado"] != true {
		t.Errorf("gravado: %v", n)
	}
	if status, _ := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_nome": true}); status != http.StatusOK {
		t.Errorf("com gravação: %d", status)
	}
	// Esquecer as gravações desliga o ouvir o nome; desligar os comandos também.
	if _, n := pedir(t, "DELETE", srv.URL+"/v1/perfis/1/voz/nome", nil); n["gravado"] != false {
		t.Errorf("depois de esquecer: %v", n)
	}
	if p := perfilDe(t, srv.URL, 1); p["voz_nome"] != false {
		t.Errorf("continuou ouvindo: %v", p)
	}
	pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"mascote_nome": "Colmeia", "voz_nome": true})
	pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_comandos": false})
	if p := perfilDe(t, srv.URL, 1); p["voz_nome"] != false || p["voz_comandos"] != false {
		t.Errorf("desligar os comandos: %v", p)
	}
	// Apagar o Whisper desliga os comandos de todos.
	pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_comandos": true})
	pedir(t, "DELETE", srv.URL+"/v1/voz/modelo", nil)
	if p := perfilDe(t, srv.URL, 1); p["voz_comandos"] != false {
		t.Errorf("comandos sem o modelo: %v", p)
	}
}
