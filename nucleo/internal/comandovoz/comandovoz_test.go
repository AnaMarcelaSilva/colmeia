package comandovoz

import (
	"fmt"
	"strings"
	"testing"
)

func TestEntendeAsFrasesComoOWhisperEscreve(t *testing.T) {
	casos := []struct {
		frase    string
		intencao Intencao
		alvo     string
	}{
		{"Rodar loja-web.", Rodar, "loja web"},
		{"rodar a loja web", Rodar, "loja web"},
		{"Roda o projeto API pedidos, por favor.", Rodar, "api pedidos"},
		{"Colmeia, rode a api de pedidos", Rodar, "api de pedidos"},
		{"Ei Colmeia, dá play na loja web", Rodar, "loja web"},
		{"Executar estudos Rust", Rodar, "estudos rust"},
		{"Parar a loja web!", Parar, "loja web"},
		{"Pare o projeto api-pedidos", Parar, "api pedidos"},
		{"Desliga a loja web agora", Parar, "loja web"},
		{"Encerrar a tarefa login com Google.", Encerrar, "login com google"},
		{"Encerra a tarefa de login", Encerrar, "login"},
		{"Conclui a tarefa Ajustar o cache", Encerrar, "ajustar o cache"},
		{"Finalizar a tarefa testes de integração", Encerrar, "testes de integracao"},
		{"Abrir a tarefa login com Google", Abrir, "login com google"},
		{"Abra a tarefa do cache", Abrir, "cache"},
		{"Me mostra a tarefa de testes", Abrir, "testes"},
		{"Vai para a tarefa login", Abrir, "login"},
		{"Como está a tarefa login com Google?", ComoEsta, "login com google"},
		{"Como que está a tarefa do cache?", ComoEsta, "cache"},
		{"Como anda o login?", ComoEsta, "login"},
		{"Qual o status da tarefa de testes?", ComoEsta, "testes"},
		{"Status do login com Google", ComoEsta, "login com google"},
		{"O que espera por mim?", OQueEspera, ""},
		{"O que está esperando por mim?", OQueEspera, ""},
		{"Tem alguma coisa esperando por mim?", OQueEspera, ""},
		{"O que precisa de mim?", OQueEspera, ""},
		{"Colmeia, o que tem pra mim?", OQueEspera, ""},
		{"Sim.", Sim, ""},
		{"Pode sim!", Sim, ""},
		{"Confirmo", Sim, ""},
		{"Não.", Nao, ""},
		{"Cancela", Nao, ""},
		{"Deixa pra lá", Nao, ""},
		{"Codar a loja webs", Rodar, "loja webs"},
		{"Cudar a loja webs", Rodar, "loja webs"},
		{"Abrirr a tarefa de login", Abrir, "login"},
		{"e eu quis esperar por mim.", OQueEspera, ""},
		{"Colmeia, sim.", Sim, ""},
		{"Comeia.", SoNome, ""},
		{"Com meia, rodar a loja web.", Rodar, "loja web"},
		{"Colmeias, o que espera por mim?", OQueEspera, ""},
		{"Qual a previsão do tempo?", Desconhecida, ""},
		{"", Desconhecida, ""},
		{"Rodar", Desconhecida, ""},
		{"[Música]", Desconhecida, ""},
		{"Obrigada", Desconhecida, ""},
	}
	for _, c := range casos {
		p := Entender(c.frase, "Colmeia")
		if p.Intencao != c.intencao || p.Alvo != c.alvo {
			t.Errorf("%q: veio %v %q, esperava %v %q", c.frase, p.Intencao, p.Alvo, c.intencao, c.alvo)
		}
	}
}

func TestVerboCorrigidoSempreConfirma(t *testing.T) {
	p := Entender("Cudar a loja webs", "Colmeia")
	if !p.Duvida {
		t.Fatalf("%+v", p)
	}
	r := Responder(p, quadro())
	if !r.Confirmar || r.Acao == nil || r.Texto != "Rodar o projeto loja-web?" {
		t.Errorf("%+v", r)
	}
	// "mudar" está a duas letras de "rodar": também só com o sim.
	if r := Responder(Entender("mudar a loja web", "Colmeia"), quadro()); r.Acao != nil && !r.Confirmar {
		t.Errorf("mudar: %+v", r)
	}
	// Sem correção, a mesma frase roda na hora.
	if r := Responder(Entender("rodar a loja web", "Colmeia"), quadro()); r.Confirmar {
		t.Errorf("%+v", r)
	}
}

func TestONomeDaAbelhaDoPerfilSaiDoComeco(t *testing.T) {
	if p := Entender("Zuzu, rodar a loja web", "Zuzu"); p.Intencao != Rodar || p.Alvo != "loja web" {
		t.Errorf("%+v", p)
	}
	// Sem o nome, a frase vale do mesmo jeito.
	if p := Entender("rodar a loja web", "Zuzu"); p.Intencao != Rodar {
		t.Errorf("%+v", p)
	}
	// Um nome curto não aceita letras trocadas: "zuza" não é "zuzu".
	if p := Entender("zuza rodar a loja web", "Zuzu"); p.Intencao != Desconhecida {
		t.Errorf("%+v", p)
	}
}

func TestAchaNomesParecidos(t *testing.T) {
	cands := []Candidato{{ID: 1, Nome: "loja-web"}, {ID: 2, Nome: "api-pedidos"}, {ID: 3, Nome: "estudos-rust"}}
	casos := map[string]int64{
		"loja web":      1,
		"lojaweb":       1,
		"loja wab":      1,
		"api pedidos":   2,
		"a pi pedidos":  2,
		"api de pedido": 2,
		"estudos rust":  3,
		"estudo rasti":  3,
	}
	for dito, id := range casos {
		m, ok := Procurar(dito, cands).Melhor()
		if !ok || m.ID != id {
			t.Errorf("%q: veio %+v (%v), esperava %d", dito, m, ok, id)
		}
	}
	if _, ok := Procurar("banco de dados financeiro", cands).Melhor(); ok {
		t.Error("achou algo para um nome que não existe")
	}
	if !Procurar("loja web", cands).Certo() {
		t.Error("loja web devia ser certo")
	}
	// Parte de um título longo acha, mas sem a certeza de dizer o nome todo.
	titulos := []Candidato{{ID: 7, Nome: "Login com Google no app"}, {ID: 8, Nome: "Ajustar o cache de imagens"}}
	b := Procurar("login", titulos)
	if m, ok := b.Melhor(); !ok || m.ID != 7 {
		t.Errorf("login: %+v", b)
	}
	if m, _ := Procurar("ajustar cache", titulos).Melhor(); m.ID != 8 {
		t.Errorf("ajustar cache: %+v", m)
	}
}

func TestEmpateNaoEscolheSozinho(t *testing.T) {
	cands := []Candidato{{ID: 1, Nome: "Testes do login"}, {ID: 2, Nome: "Testes do cadastro"}}
	if e := Procurar("testes", cands).Empate(); len(e) != 2 {
		t.Errorf("devia empatar: %+v", e)
	}
	if e := Procurar("testes do login", cands).Empate(); e != nil {
		t.Errorf("não devia empatar: %+v", e)
	}
}

func quadro() Quadro {
	return Quadro{
		Projetos: []Projeto{
			{ID: 1, Nome: "loja-web", Execucoes: []Execucao{{ID: 11, Nome: "servidor"}}},
			{ID: 2, Nome: "api-pedidos", Execucoes: []Execucao{{ID: 21, Nome: "servidor", Rodando: true}, {ID: 22, Nome: "testes"}}},
			{ID: 3, Nome: "estudos-rust"},
		},
		Tarefas: []Tarefa{
			{ID: 101, ProjetoID: 1, Titulo: "Login com Google", Coluna: "trabalhando"},
			{ID: 102, ProjetoID: 1, Titulo: "Ajustar o cache", Coluna: "aguardando"},
			{ID: 103, ProjetoID: 2, Titulo: "Cupom de desconto", Coluna: "concluido"},
			{ID: 104, ProjetoID: 2, Titulo: "Testes de integração", Coluna: "revisao"},
		},
		Agentes: []Agente{
			{ID: 1, TarefaID: 101, Nome: "Claude Code", Ativo: true, Estado: "aguardando", Motivo: "pede aprovação"},
			{ID: 2, TarefaID: 101, Nome: "Codex", Ativo: true, Estado: "trabalhando"},
			{ID: 3, TarefaID: 104, Nome: "Claude Code", Erro: true},
		},
	}
}

func responder(frase string) Resposta {
	return Responder(Entender(frase, "Colmeia"), quadro())
}

func TestRodarProjetoComUmaConfiguracaoRodaNaHora(t *testing.T) {
	r := responder("Rodar a loja web")
	if r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoRodar || r.Acao.ExecucaoID != 11 {
		t.Fatalf("%+v", r)
	}
	if r.Texto != "Rodando o projeto loja-web." {
		t.Errorf("%q", r.Texto)
	}
}

func TestRodarProjetoComVariasConfiguracoesPerguntaQual(t *testing.T) {
	r := responder("rodar a api pedidos")
	if r.Acao != nil || !strings.Contains(r.Texto, "servidor e testes") {
		t.Fatalf("%+v", r)
	}
	r = responder("rodar os testes da api pedidos")
	if r.Acao == nil || r.Acao.ExecucaoID != 22 {
		t.Fatalf("%+v", r)
	}
}

func TestRodarOQueJaRodaPedeConfirmacao(t *testing.T) {
	r := responder("rodar o servidor da api pedidos")
	if !r.Confirmar || r.Acao == nil || r.Acao.ExecucaoID != 21 || !strings.Contains(r.Texto, "já está rodando") {
		t.Fatalf("%+v", r)
	}
}

func TestProjetoSemConfiguracaoESemNome(t *testing.T) {
	if r := responder("rodar estudos rust"); r.Acao != nil || !strings.Contains(r.Texto, "ainda não tem o que rodar") {
		t.Errorf("%+v", r)
	}
	if r := responder("rodar o financeiro"); r.Acao != nil || !strings.HasPrefix(r.Texto, "Não achei") {
		t.Errorf("%+v", r)
	}
}

func TestPararSempreConfirmaENaoParaOQueNaoRoda(t *testing.T) {
	r := responder("parar o servidor da api pedidos")
	if !r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoParar || r.Acao.ExecucaoID != 21 {
		t.Fatalf("%+v", r)
	}
	if r := responder("parar a loja web"); r.Acao != nil || !strings.Contains(r.Texto, "não está rodando") {
		t.Errorf("%+v", r)
	}
}

func TestEncerrarSempreConfirma(t *testing.T) {
	r := responder("encerrar a tarefa login com google")
	if !r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoEncerrar || r.Acao.TarefaID != 101 {
		t.Fatalf("%+v", r)
	}
	if !strings.Contains(r.Acao.Feito, "Concluído") {
		t.Errorf("%q", r.Acao.Feito)
	}
	if r := responder("encerrar o cupom de desconto"); r.Acao != nil || !strings.Contains(r.Texto, "já está em Concluído") {
		t.Errorf("%+v", r)
	}
}

func TestAbrirComCertezaAbreSemPerguntar(t *testing.T) {
	r := responder("abrir a tarefa login com google")
	if r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoAbrir || r.Acao.TarefaID != 101 {
		t.Fatalf("%+v", r)
	}
	// Um nome só parecido pergunta antes.
	r = responder("abrir a tarefa logim com gugou")
	if r.Acao == nil || r.Acao.TarefaID != 101 || !r.Confirmar {
		t.Fatalf("%+v", r)
	}
}

func TestComoEstaContaOsAgentes(t *testing.T) {
	r := responder("como está a tarefa login com google?")
	want := "A tarefa Login com Google está com o agente trabalhando. Um agente espera você: Claude Code, que pede aprovação. Um agente trabalhando: Codex."
	if r.Texto != want || r.Acao != nil {
		t.Errorf("\n%q\n%q", r.Texto, want)
	}
	r = responder("como está a tarefa testes de integração")
	if !strings.Contains(r.Texto, "em Revisão") || !strings.Contains(r.Texto, "Um parou com erro: Claude Code.") {
		t.Errorf("%q", r.Texto)
	}
	if r := responder("como está o cache"); !strings.Contains(r.Texto, "aguardando você") || !strings.Contains(r.Texto, "ainda não tem agentes") {
		t.Errorf("%q", r.Texto)
	}
}

func TestOQueEsperaPorMim(t *testing.T) {
	r := responder("o que espera por mim?")
	want := "Duas tarefas esperam você: Login com Google, pede aprovação; e Ajustar o cache."
	if r.Texto != want {
		t.Errorf("\n%q\n%q", r.Texto, want)
	}
	if r := Responder(Pedido{Intencao: OQueEspera}, Quadro{}); r.Texto != "Nada espera você agora." {
		t.Errorf("%q", r.Texto)
	}
}

func TestSemEntenderDizOQueDaParaPedir(t *testing.T) {
	for _, f := range []string{"qual a previsão do tempo", "faz um café", ""} {
		if r := responder(f); r.Texto != Ajuda || r.Acao != nil {
			t.Errorf("%q: %+v", f, r)
		}
	}
}

func TestTarefaQueNaoExiste(t *testing.T) {
	if r := responder("abrir a tarefa relatório financeiro"); r.Acao != nil || !strings.HasPrefix(r.Texto, "Não achei") {
		t.Errorf("%+v", r)
	}
}

func TestListaEmPortugues(t *testing.T) {
	casos := map[string][]string{
		"":                    nil,
		"a":                   {"a"},
		"a e b":               {"a", "b"},
		"a, b e c":            {"a", "b", "c"},
		"a, b, c e mais dois": {"a", "b", "c", "d", "e"},
		"a, x; b; e c":        {"a, x", "b", "c"},
	}
	for want, itens := range casos {
		if got := lista(itens); got != want {
			t.Errorf("%v: %q, esperava %q", itens, got, want)
		}
	}
}

func TestSemChamado(t *testing.T) {
	casos := map[string]string{
		"Colmeia, o que eu fiz hoje?":         "o que eu fiz hoje?",
		"Ei, Colmeia! O que falta na sprint?": "O que falta na sprint?",
		"com meia quanto falta?":              "quanto falta?",
		"O que eu fiz hoje?":                  "O que eu fiz hoje?",
		"  ":                                  "",
	}
	for frase, quer := range casos {
		if got := SemChamado(frase, "Colmeia"); got != quer {
			t.Errorf("%q: %q, queria %q", frase, got, quer)
		}
	}
	if got := SemChamado("Zuzu, como foi o dia?", "Zuzu"); got != "como foi o dia?" {
		t.Errorf("outro nome: %q", got)
	}
}

func TestVocabularioTemONomeOsProjetosEAsTarefasAbertasPrimeiro(t *testing.T) {
	q := Quadro{
		Projetos: []Projeto{{ID: 1, Nome: "loja-web", Execucoes: []Execucao{{ID: 1, Nome: "servidor"}}}, {ID: 2, Nome: "api-pedidos"}},
		Tarefas: []Tarefa{
			{ID: 1, ProjetoID: 1, Titulo: "Cupom de desconto", Coluna: "concluido"},
			{ID: 2, ProjetoID: 1, Titulo: "Ajustar   o cache", Coluna: "trabalhando"},
			{ID: 3, ProjetoID: 2, Titulo: "Login com Google", Coluna: "revisao"},
		},
	}
	v := Vocabulario("Zuzu", q)
	for _, quer := range []string{"Zuzu, rodar o projeto na Colmeia.", "encerrar a tarefa", "Projetos: loja-web, servidor, api-pedidos.", "Tarefas: Ajustar o cache, Login com Google, Cupom de desconto."} {
		if !strings.Contains(v, quer) {
			t.Errorf("faltou %q em %q", quer, v)
		}
	}
	if v := Vocabulario("", Quadro{}); !strings.HasPrefix(v, "Colmeia, rodar o projeto.") || strings.Contains(v, "Projetos") || strings.Contains(v, "na Colmeia") {
		t.Errorf("sem nome e sem quadro: %q", v)
	}
}

func TestVocabularioCabeNoLimiteSemCortarNome(t *testing.T) {
	var q Quadro
	for i := 0; i < 200; i++ {
		q.Tarefas = append(q.Tarefas, Tarefa{ID: int64(i), Titulo: fmt.Sprintf("Tarefa de exemplo número %d", i), Coluna: "trabalhando"})
	}
	v := Vocabulario("Colmeia", q)
	if len(v) > LimiteVocabulario {
		t.Fatalf("passou do limite: %d", len(v))
	}
	if !strings.HasSuffix(v, ".") || strings.HasSuffix(v, ", .") {
		t.Errorf("o fim ficou cortado: %q", v[len(v)-40:])
	}
	ultima := v[strings.LastIndex(v, ", ")+2 : len(v)-1]
	if !strings.HasPrefix(ultima, "Tarefa de exemplo número ") {
		t.Errorf("a última tarefa ficou pela metade: %q", ultima)
	}
}

func TestMotivoDoAgenteSemOQueSobrando(t *testing.T) {
	if s := comMotivo(Agente{Nome: "Claude Code", Motivo: "esperando resposta"}); s != "Claude Code, esperando resposta" {
		t.Errorf("%q", s)
	}
	if s := comMotivo(Agente{Nome: "Claude Code", Motivo: "pede aprovação"}); s != "Claude Code, que pede aprovação" {
		t.Errorf("%q", s)
	}
}
