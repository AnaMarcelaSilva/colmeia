//go:build unix

package pergunta

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
)

func retratoDeTeste() Contexto {
	return Contexto{Agora: "16/10/2026 18:00", Tarefas: []Tarefa{
		{Titulo: "Relatório de vendas", Projeto: "loja-web", Estado: "em revisão", Agentes: []Agente{{Nome: "Claude Code", Estado: "parado"}},
			FraseDaily: "O relatório ficou pronto."},
	}, Hoje: []Evento{{Hora: "10:02", Texto: "Concluiu “Relatório de vendas”."}}}
}

// falso escreve um executável que guarda a entrada e os argumentos e
// responde `resultado` no envelope (depois de `espera` segundos).
func falso(t *testing.T, resultado string, espera string) (string, string) {
	t.Helper()
	dir := t.TempDir()
	env, _ := json.Marshal(map[string]any{"type": "result", "subtype": "success", "is_error": false, "result": resultado})
	if err := os.WriteFile(filepath.Join(dir, "resposta.json"), env, 0o600); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\ncat > \"" + dir + "/entrada\"\nprintf '%s\\n' \"$@\" > \"" + dir + "/args\"\npwd > \"" + dir + "/pasta\"\n" +
		"sleep " + espera + "\ncat \"" + dir + "/resposta.json\"\n"
	exe := filepath.Join(dir, "claude")
	if err := os.WriteFile(exe, []byte(script), 0o700); err != nil {
		t.Fatal(err)
	}
	return exe, dir
}

func ler(t *testing.T, dir, nome string) string {
	t.Helper()
	b, _ := os.ReadFile(filepath.Join(dir, nome))
	return string(b)
}

func TestResponderComExecutavelFalso(t *testing.T) {
	exe, dir := falso(t, "**Hoje** você concluiu o `Relatório de vendas`.\nNada espera você.", "0")
	r, err := Perguntador{Executavel: exe}.Responder(context.Background(), "  o que eu fiz hoje? ", retratoDeTeste())
	if err != nil {
		t.Fatal(err)
	}
	if r != "Hoje você concluiu o Relatório de vendas. Nada espera você." {
		t.Errorf("resposta: %q", r)
	}
	entrada := ler(t, dir, "entrada")
	if !strings.Contains(entrada, "<pergunta>\no que eu fiz hoje?\n</pergunta>") || !strings.Contains(entrada, `"frase_daily":"O relatório ficou pronto."`) ||
		!strings.Contains(entrada, `"o_que_aconteceu_hoje":[{"hora":"10:02"`) {
		t.Errorf("entrada: %s", entrada)
	}
	if ler(t, dir, "args") != strings.Join(claudecode.Argumentos, "\n")+"\n" {
		t.Errorf("argumentos: %q", ler(t, dir, "args"))
	}
	pasta := strings.TrimSpace(ler(t, dir, "pasta"))
	if _, err := os.Stat(pasta); !strings.Contains(pasta, "colmeia-pergunta-") || !os.IsNotExist(err) {
		t.Errorf("pasta %q ficou: %v", pasta, err)
	}
}

func TestPrazoDaPergunta(t *testing.T) {
	antes := Prazo
	Prazo = 300 * time.Millisecond
	t.Cleanup(func() { Prazo = antes })
	exe, _ := falso(t, "tarde demais", "3")
	inicio := time.Now()
	_, err := Perguntador{Executavel: exe}.Responder(context.Background(), "como foi o dia?", retratoDeTeste())
	if err == nil || !strings.Contains(err.Error(), "não respondeu") || time.Since(inicio) > 2900*time.Millisecond {
		t.Errorf("prazo: %v em %s", err, time.Since(inicio))
	}
}

func TestPerguntaConferida(t *testing.T) {
	for _, ruim := range []string{"", "  ", strings.Repeat("a", MaxPergunta+1), "a senha: hunter22222 serve?", "a\x00b"} {
		if _, err := Conferir(ruim); err == nil {
			t.Errorf("aceitou %q", ruim)
		}
	}
	// Uma pergunta ruim não chega a rodar nada.
	if _, err := (Perguntador{Executavel: "/nao/existe"}).Responder(context.Background(), "", Contexto{}); err == nil || strings.Contains(err.Error(), "exec") {
		t.Errorf("pergunta vazia: %v", err)
	}
}

func TestRespostaValidada(t *testing.T) {
	for nome, texto := range map[string]string{
		"vazia":       " \n ",
		"com senha":   "A senha do banco é: hunter22222",
		"grande":      strings.Repeat("palavra ", 300),
		"de controle": "tudo certo\x1b[31m",
	} {
		if r, err := Validar(texto); err == nil {
			t.Errorf("%s: aceitou %q", nome, r)
		}
	}
	// Um pouco grande: corta no fim da última frase que cabe.
	longa := strings.Repeat("Uma frase curta e falável. ", 30)
	r, err := Validar(longa)
	if err != nil || len([]rune(r)) > MaxResposta || !strings.HasSuffix(r, ".") {
		t.Errorf("cortar: %q %v", r, err)
	}
	if r, _ := Validar("# Título\n- um\n- dois"); r != "Título - um - dois" {
		t.Errorf("markdown: %q", r)
	}
}

func TestLimpoTiraSegredos(t *testing.T) {
	if Limpo("token: ghp_abcdefghijklmnopqrstuvwxyz0123456789") != "" || Limpo("linha\x1b[0m") != "" {
		t.Error("segredo ou controle passou")
	}
	if got := Limpo("  duas\n linhas "); got != "duas linhas" {
		t.Errorf("limpo: %q", got)
	}
	if got := Limpo(strings.Repeat("x", MaxTexto+10)); len([]rune(got)) != MaxTexto {
		t.Errorf("cortar: %d", len([]rune(got)))
	}
}
