//go:build unix

package proposta

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func sprintDeTeste() Sprint {
	return Sprint{Inicio: "2026-10-09", Fim: "2026-10-15", Assuntos: []string{"Vendas"}, Tarefas: []Tarefa{
		{ID: 114, Titulo: "Criar relatório de vendas", Projeto: "loja-web", Estado: "Concluída", Descricao: "Relatório por período.", Assunto: "Vendas"},
		{ID: 117, Titulo: "Documentar API de pedidos", Projeto: "api-pedidos", Estado: "Em revisão", Feito: []string{"Claude Code terminou uma sessão."}},
	}}
}

// envelope é a saída do Claude Code com --output-format json.
func envelope(t *testing.T, resultado string) string {
	t.Helper()
	b, _ := json.Marshal(map[string]any{"type": "result", "subtype": "success", "is_error": false, "result": resultado, "total_cost_usd": 0.01})
	return string(b)
}

// falso escreve um executável que guarda a entrada, os argumentos e a pasta
// em que rodou, e responde `saida` (depois de `espera` segundos).
func falso(t *testing.T, saida string, espera int) (string, string) {
	t.Helper()
	dir := t.TempDir()
	resposta := filepath.Join(dir, "resposta.json")
	if err := os.WriteFile(resposta, []byte(saida), 0o600); err != nil {
		t.Fatal(err)
	}
	script := "#!/bin/sh\n" +
		"cat > \"" + dir + "/entrada\"\n" +
		"printf '%s\\n' \"$@\" > \"" + dir + "/args\"\n" +
		"pwd > \"" + dir + "/pasta\"\n" +
		"ls -A > \"" + dir + "/conteudo\"\n" +
		"sleep " + itoaN(espera) + "\n" +
		"cat \"" + resposta + "\"\n"
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

func TestGerarComExecutavelFalso(t *testing.T) {
	proposta := `{"itens":[{"tarefa_id":114,"titulo":"Relatório de vendas por loja","descricao":"O financeiro já usa no fechamento.","assunto":"Clientes da loja"},` +
		`{"tarefa_id":117,"titulo":"Pedidos documentados","descricao":"O time do aplicativo usa sem perguntar.","assunto":""}]}`
	exe, dir := falso(t, envelope(t, proposta), 0)
	g := Gerador{Executavel: exe, Env: []string{"CLAUDE_CONFIG_DIR=/tmp/conta-teste"}}
	p, err := g.Gerar(context.Background(), "  agrupe por cliente e escreva para o gerente ", sprintDeTeste())
	if err != nil {
		t.Fatal(err)
	}
	if len(p.Itens) != 2 || p.Itens[0].Assunto != "Clientes da loja" || p.Itens[1].Assunto != "" {
		t.Fatalf("proposta: %+v", p)
	}
	// Um pedido só, pela entrada, com o pedido e os dados da sprint em JSON.
	entrada := ler(t, dir, "entrada")
	if !strings.Contains(entrada, "<pedido>\nagrupe por cliente e escreva para o gerente\n</pedido>") ||
		!strings.Contains(entrada, `"titulo":"Criar relatório de vendas"`) || !strings.Contains(entrada, `"o_que_aconteceu":["Claude Code terminou uma sessão."]`) {
		t.Errorf("entrada: %s", entrada)
	}
	// Sem ferramentas, sem MCP, sem configurações e sem conversa guardada.
	args := ler(t, dir, "args")
	if args != strings.Join(Argumentos, "\n")+"\n" || !strings.Contains(args, "--tools\n\n") {
		t.Errorf("argumentos: %q", args)
	}
	// Numa pasta vazia, apagada depois.
	pasta := strings.TrimSpace(ler(t, dir, "pasta"))
	if ler(t, dir, "conteudo") != "" || !strings.Contains(pasta, "colmeia-proposta-") {
		t.Errorf("pasta: %q com %q", pasta, ler(t, dir, "conteudo"))
	}
	if _, err := os.Stat(pasta); !os.IsNotExist(err) {
		t.Errorf("a pasta ficou: %v", err)
	}
}

func TestRespostasRecusadas(t *testing.T) {
	s := sprintDeTeste()
	casos := map[string]string{
		"texto solto":         "Claro! Aqui está.",
		"campo desconhecido":  `{"itens":[{"tarefa_id":114,"titulo":"a","descricao":"b","assunto":"","extra":1}]}`,
		"texto depois":        `{"itens":[{"tarefa_id":114,"titulo":"a","descricao":"b","assunto":""}]} pronto`,
		"tarefa de fora":      `{"itens":[{"tarefa_id":999,"titulo":"a","descricao":"b","assunto":""}]}`,
		"repetida":            `{"itens":[{"tarefa_id":114,"titulo":"a","descricao":"b","assunto":""},{"tarefa_id":114,"titulo":"a","descricao":"b","assunto":""}]}`,
		"título vazio":        `{"itens":[{"tarefa_id":114,"titulo":" ","descricao":"b","assunto":""}]}`,
		"título com quebra":   `{"itens":[{"tarefa_id":114,"titulo":"a\nb","descricao":"b","assunto":""}]}`,
		"título grande":       `{"itens":[{"tarefa_id":114,"titulo":"` + strings.Repeat("a", MaxTitulo+1) + `","descricao":"b","assunto":""}]}`,
		"descrição com senha": `{"itens":[{"tarefa_id":114,"titulo":"a","descricao":"senha: hunter22222","assunto":""}]}`,
		"vazia":               `{"itens":[]}`,
	}
	for nome, texto := range casos {
		if _, err := Validar(texto, s); err == nil {
			t.Errorf("%s: aceitou %s", nome, texto)
		}
	}
	// O bloco de código em volta é o único desvio aceito.
	p, err := Validar("```json\n{\"itens\":[{\"tarefa_id\":117,\"titulo\":\"a\",\"descricao\":\"b\",\"assunto\":\" X \"}]}\n```", s)
	if err != nil || p.Itens[0].Assunto != "X" {
		t.Errorf("com cercas: %+v %v", p, err)
	}
	// Assuntos demais.
	grande := Sprint{Tarefas: []Tarefa{}}
	for i := 0; i <= MaxAssuntos; i++ {
		grande.Tarefas = append(grande.Tarefas, Tarefa{ID: int64(i + 1)})
	}
	var muitos []string
	for i := 0; i <= MaxAssuntos; i++ {
		muitos = append(muitos, `{"tarefa_id":`+itoaN(i+1)+`,"titulo":"a","descricao":"b","assunto":"A`+string(rune('a'+i))+`"}`)
	}
	if _, err := Validar(`{"itens":[`+strings.Join(muitos, ",")+`]}`, grande); err == nil {
		t.Error("aceitou assuntos demais")
	}
}

func itoaN(n int) string {
	b, _ := json.Marshal(n)
	return string(b)
}

func TestEnvelopeComErroEPrazo(t *testing.T) {
	s := sprintDeTeste()
	erro, _ := json.Marshal(map[string]any{"type": "result", "subtype": "success", "is_error": true, "result": "Credit balance is too low"})
	if _, err := Ler(erro, s); err == nil || !strings.Contains(err.Error(), "Credit balance is too low") {
		t.Errorf("erro do Claude Code: %v", err)
	}
	if _, err := Ler([]byte("não é json"), s); err == nil {
		t.Error("aceitou saída sem envelope")
	}

	antes := Prazo
	Prazo = 300 * time.Millisecond
	t.Cleanup(func() { Prazo = antes })
	exe, _ := falso(t, envelope(t, `{"itens":[]}`), 3)
	inicio := time.Now()
	_, err := Gerador{Executavel: exe}.Gerar(context.Background(), "agrupe por cliente", s)
	if err == nil || !strings.Contains(err.Error(), "não respondeu") || time.Since(inicio) > 2900*time.Millisecond {
		t.Errorf("prazo: %v em %s", err, time.Since(inicio))
	}
}

func TestPedidoConferido(t *testing.T) {
	for _, ruim := range []string{"", "   ", strings.Repeat("a", MaxPedido+1), "use a senha: hunter22222", "a\x00b"} {
		if _, err := ConferirPedido(ruim); err == nil {
			t.Errorf("aceitou %q", ruim)
		}
	}
	if p, err := ConferirPedido(" agrupe por cliente\ne escreva para o gerente "); err != nil || p != "agrupe por cliente\ne escreva para o gerente" {
		t.Errorf("pedido bom: %q %v", p, err)
	}
	if _, err := (Gerador{Executavel: "/nao/existe"}).Gerar(context.Background(), "x", Sprint{}); err == nil {
		t.Error("sprint sem tarefas passou")
	}
	if r := Resumir([]string{strings.Repeat("x", 300), "b", "c", "d", "e", "f", "g", "h", "i"}); len(r) != maxFeito || len([]rune(r[0])) != maxTextoFeito {
		t.Errorf("resumir: %d %d", len(r), len([]rune(r[0])))
	}
}
