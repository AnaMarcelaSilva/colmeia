//go:build unix

package api

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/comandovoz"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/pergunta"
)

// claudeQueConta põe no PATH um "claude" que guarda a entrada, conta as
// chamadas e responde `resultado` depois de `espera` segundos.
func claudeQueConta(t *testing.T, resultado, espera string) string {
	t.Helper()
	dir := t.TempDir()
	env, _ := json.Marshal(map[string]any{"type": "result", "subtype": "success", "is_error": false, "result": resultado})
	if err := os.WriteFile(filepath.Join(dir, "resposta"), env, 0o600); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\ncat > \"" + dir + "/entrada\"\necho x >> \"" + dir + "/chamadas\"\nprintf '%s\\n' \"$@\" > \"" + dir + "/args\"\n" +
		"sleep " + espera + "\ncat \"" + dir + "/resposta\"\n"
	if err := os.WriteFile(filepath.Join(dir, "claude"), []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	t.Setenv("PATH", dir+":/usr/bin:/bin")
	return dir
}

func chamadas(dir string) int {
	b, _ := os.ReadFile(filepath.Join(dir, "chamadas"))
	return strings.Count(string(b), "x")
}

func TestPerguntaLivreComIA(t *testing.T) {
	dir := t.TempDir()
	banco, err := dados.Abrir(filepath.Join(dir, "dados"))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { banco.Fechar() })
	servidor := &Servidor{Bytes: new(atomic.Int64), Banco: banco, DirDados: dir, Versao: "teste", Tempos: temposDeTeste}
	srv := httptest.NewServer(servidor.Rotas())
	t.Cleanup(func() { servidor.Agentes.FecharTodos(); servidor.Encerrar() })
	t.Cleanup(srv.Close)
	ctx := context.Background()

	perfilComAgente(t, srv.URL)
	// O que nunca pode ir: a mensagem ao agente e a pasta do projeto.
	pedir(t, "POST", srv.URL+"/v1/agentes/1/mensagens", map[string]string{"texto": "roda os testes xyzzy"})
	projetos, err := banco.ListarProjetos(ctx, 1)
	if err != nil || len(projetos) != 1 || projetos[0].Caminho == "" {
		t.Fatalf("projeto: %v %v", projetos, err)
	}
	pasta := projetos[0].Caminho
	if _, err := banco.GravarNota(ctx, dados.GravacaoNota{Tarefa: 1, Tipo: "daily", Periodo: time.Now().Format("2006-01-02"), Texto: "O relatório ficou pronto."}); err != nil {
		t.Fatal(err)
	}
	falar := func(frase string) (int, map[string]any) {
		return pedir(t, "POST", srv.URL+"/v1/perfis/1/voz/comando", map[string]string{"texto": frase})
	}

	// Tudo desligado: nada é aceito e nada é chamado.
	claude := claudeQueConta(t, "Hoje você criou a tarefa Analisar relatório.", "0")
	if status, _ := falar("o que eu fiz hoje?"); status != http.StatusBadRequest || chamadas(claude) != 0 {
		t.Fatalf("desligado: %d, %d chamadas", status, chamadas(claude))
	}
	// Só os comandos: o que não é comando ganha a ajuda, sem chamar o Claude Code.
	if err := banco.DefinirVoz(ctx, 1, dados.OpcoesVoz{Comandos: true, Sensibilidade: 5}); err != nil {
		t.Fatal(err)
	}
	if _, r := falar("o que eu fiz hoje?"); r["texto"] != comandovoz.Ajuda || chamadas(claude) != 0 {
		t.Fatalf("opção desligada: %v, %d chamadas", r, chamadas(claude))
	}

	// Ligar sem o Claude Code instalado explica; com ele, liga.
	t.Setenv("PATH", t.TempDir()+":/usr/bin:/bin")
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_ia": true}); status != http.StatusBadRequest || !strings.Contains(r["erro"].(string), "não está instalado") {
		t.Fatalf("ligou sem o Claude Code: %d %v", status, r)
	}
	t.Setenv("PATH", claude+":/usr/bin:/bin")
	if status, r := pedir(t, "PATCH", srv.URL+"/v1/perfis/1", map[string]any{"voz_ia": true}); status != http.StatusOK || perfilDe(t, srv.URL, 1)["voz_ia"] != true {
		t.Fatalf("ligar: %d %v", status, r)
	}
	eventosAntes, _ := banco.ListarEventos(ctx, dados.FiltroEventos{Perfil: 1})

	// Ligada: a pergunta vai, sem o nome da abelha, com o retrato e só ele.
	if _, r := falar("Colmeia, o que eu fiz hoje?"); r["texto"] != "Hoje você criou a tarefa Analisar relatório." || r["confirmar"] != nil {
		t.Fatalf("pergunta: %v", r)
	}
	entrada, _ := os.ReadFile(filepath.Join(claude, "entrada"))
	texto := string(entrada)
	for _, quer := range []string{"<pergunta>\no que eu fiz hoje?\n</pergunta>", `"titulo":"Analisar relatório"`, `"projeto":"cliente-x"`,
		`"frase_daily":"O relatório ficou pronto."`, `"nome":"Terminal"`, "Criou a tarefa"} {
		if !strings.Contains(texto, quer) {
			t.Errorf("faltou %q em %s", quer, texto)
		}
	}
	for _, nunca := range []string{"xyzzy", pasta, "roda os testes", "conexao", "senha"} {
		if strings.Contains(texto, nunca) {
			t.Errorf("o retrato levou %q: %s", nunca, texto)
		}
	}
	if args, _ := os.ReadFile(filepath.Join(claude, "args")); !strings.Contains(string(args), "--tools\n\n") || !strings.Contains(string(args), "--no-session-persistence") {
		t.Errorf("argumentos: %q", args)
	}
	// Nada gravado: nenhum evento novo.
	if depois, _ := banco.ListarEventos(ctx, dados.FiltroEventos{Perfil: 1}); len(depois) != len(eventosAntes) {
		t.Errorf("a pergunta gravou %d eventos", len(depois)-len(eventosAntes))
	}
	// Um comando fixo continua sem IA.
	if _, r := falar("o que espera por mim?"); chamadas(claude) != 1 || !strings.Contains(r["texto"].(string), "espera") {
		t.Errorf("comando fixo chamou o Claude Code: %v, %d chamadas", r, chamadas(claude))
	}

	// Só a pergunta livre ligada (escrita no resumo): aceita.
	if err := banco.DefinirVoz(ctx, 1, dados.OpcoesVoz{PerguntaIA: true, Sensibilidade: 5}); err != nil {
		t.Fatal(err)
	}
	if status, r := falar("e amanhã?"); status != http.StatusOK || chamadas(claude) != 2 {
		t.Errorf("escrita: %d %v", status, r)
	}

	// Sem o Claude Code, a abelha explica no balão.
	t.Setenv("PATH", t.TempDir()+":/usr/bin:/bin")
	if _, r := falar("como foi o dia?"); !strings.Contains(r["texto"].(string), "Claude Code instalado") {
		t.Errorf("sem Claude Code: %v", r)
	}
	// Uma resposta com cara de senha não passa.
	claude = claudeQueConta(t, "A senha: hunter22222", "0")
	if _, r := falar("qual a senha?"); !strings.HasPrefix(r["texto"].(string), "Não consegui responder") {
		t.Errorf("resposta com senha: %v", r)
	}
	// O prazo.
	antes := pergunta.Prazo
	pergunta.Prazo = 300 * time.Millisecond
	t.Cleanup(func() { pergunta.Prazo = antes })
	claudeQueConta(t, "tarde", "3")
	inicio := time.Now()
	if _, r := falar("como foi o dia?"); !strings.Contains(r["texto"].(string), "não respondeu") || time.Since(inicio) > 2900*time.Millisecond {
		t.Errorf("prazo: %v em %s", r, time.Since(inicio))
	}
}
