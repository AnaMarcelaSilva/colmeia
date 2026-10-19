package comandovoz

import (
	"fmt"
	"strings"
	"testing"
)

func TestListaComOQueEsperaPrimeiroEOrdemEstavel(t *testing.T) {
	q := quadro()
	// Mais tarefas: uma esperando sem agente, uma trabalhando, uma concluída com erro.
	q.Tarefas = append(q.Tarefas,
		Tarefa{ID: 105, ProjetoID: 1, Titulo: "Página de produto", Coluna: "trabalhando"},
		Tarefa{ID: 99, ProjetoID: 2, Titulo: "Relatório mensal", Coluna: "aguardando"},
	)
	q.Agentes = append(q.Agentes,
		Agente{ID: 4, TarefaID: 105, Nome: "Codex", Ativo: true, Estado: "trabalhando"},
		Agente{ID: 5, TarefaID: 103, Nome: "Claude Code", Erro: true},
	)
	itens := MontarLista(q)
	var linhas []string
	for n, i := range itens {
		linhas = append(linhas, i.Linha(n+1))
	}
	quer := []string{
		"1. Relatório mensal: espera você.",
		"2. Login com Google: o Claude Code pede aprovação.",
		"3. Ajustar o cache: espera você.",
		"4. Página de produto: trabalhando.",
		"5. Testes de integração: o Claude Code parou com erro.",
	}
	if strings.Join(linhas, "\n") != strings.Join(quer, "\n") {
		t.Fatalf("lista:\n%s", strings.Join(linhas, "\n"))
	}
	if itens[1].AgenteID != 1 || itens[3].AgenteID != 4 || itens[0].AgenteID != 0 {
		t.Errorf("agentes: %+v", itens)
	}
	// A mesma lista com o quadro em outra ordem.
	q.Tarefas[0], q.Tarefas[len(q.Tarefas)-1] = q.Tarefas[len(q.Tarefas)-1], q.Tarefas[0]
	for n, i := range MontarLista(q) {
		if i.TarefaID != itens[n].TarefaID {
			t.Fatalf("a ordem mudou com o quadro: %+v", MontarLista(q))
		}
	}
}

func TestPaginasDaLista(t *testing.T) {
	var q Quadro
	for i := range 7 {
		q.Tarefas = append(q.Tarefas, Tarefa{ID: int64(i + 1), Titulo: fmt.Sprintf("Tarefa %d", i+1), Coluna: "aguardando"})
	}
	itens := MontarLista(q)
	if Paginas(itens) != 2 || len(Pagina(itens, 0)) != 5 || len(Pagina(itens, 1)) != 2 || Pagina(itens, 2) != nil {
		t.Fatalf("páginas: %d", Paginas(itens))
	}
	fala := FalaDaLista(itens, 0)
	if !strings.HasPrefix(fala, "Página 1 de 2. 1. Tarefa 1: espera você.") || !strings.HasSuffix(fala, "Diga próxima para ouvir mais.") {
		t.Errorf("fala: %s", fala)
	}
	// Na segunda página os números recomeçam no 1.
	if fala := FalaDaLista(itens, 1); fala != "Página 2 de 2. 1. Tarefa 6: espera você. 2. Tarefa 7: espera você." {
		t.Errorf("fala 2: %s", fala)
	}
	if FalaDaLista(nil, 0) != "Nada espera você e nenhum agente está trabalhando." || Paginas(nil) != 1 {
		t.Error("vazia")
	}
}

func TestTituloLongoFicaCurtoNaLista(t *testing.T) {
	i := ItemLista{Titulo: "Refazer a tela de cadastro de clientes com validação de endereço", Detalhe: "trabalhando"}
	if l := i.Linha(1); l != "1. Refazer a tela de cadastro de clientes…: trabalhando." {
		t.Errorf("%q", l)
	}
}

func TestGramaticaComTranscricoesRuins(t *testing.T) {
	casos := []struct {
		frase  string
		verbo  Verbo
		numero int
	}{
		{"Abrir a um.", VAbrir, 1},
		{"abrir a 1", VAbrir, 1},
		{"Abrir o hum.", VAbrir, 1},
		{"A brir a um", VAbrir, 1},
		{"Abriram.", VAbrir, 1},
		{"Abre a primeira.", VAbrir, 1},
		{"Abrir a tarefa número dois.", VAbrir, 2},
		{"abrir a duas", VAbrir, 2},
		{"Colmeia, abrir a três.", VAbrir, 3},
		{"Abrir a treis", VAbrir, 3},
		{"Dois.", VAbrir, 2},
		{"Responder a um.", VResponder, 1},
		{"Responde ao 2.", VResponder, 2},
		{"Responder a primeira, por favor.", VResponder, 1},
		{"Responda à segunda", VResponder, 2},
		{"Respondê a um", VResponder, 1},
		{"Concluir a três.", VConcluir, 3},
		{"Conclui a terceira", VConcluir, 3},
		{"Concluiu a três.", VConcluir, 3},
		{"Encerrar a 2", VConcluir, 2},
		{"Enviar para revisão a quatro.", VRevisao, 4},
		{"Enviar a 4 para revisão.", VRevisao, 4},
		{"Envia pra revisão a quarta", VRevisao, 4},
		{"Revisão a um", VRevisao, 1},
		{"Parar a cinco.", VParar, 5},
		{"Para o 5", VParar, 5},
		{"Pare a quinta.", VParar, 5},
		{"Rodar a um.", VRodar, 1},
		{"Roda a primeira", VRodar, 1},
		{"Hodar a um", VRodar, 1},
		{"Próxima.", VProxima, 0},
		{"Próxima página.", VProxima, 0},
		{"Proxima", VProxima, 0},
		{"Anterior.", VAnterior, 0},
		{"Volta.", VAnterior, 0},
		{"Repetir.", VRepetir, 0},
		{"Repete, por favor.", VRepetir, 0},
		{"Fala de novo.", VRepetir, 0},
		{"Fechar.", VFechar, 0},
		{"Cancela.", VFechar, 0},
		{"Fecha", VFechar, 0},
		{"Abri a uma.", VAbrir, 1},
		{"Obrir a um", VAbrir, 1},
		{"Abrir ao 1º.", VAbrir, 1},
		{"Concluir a 3ª.", VConcluir, 3},
		{"Responderá um.", VResponder, 1},
		{"Ok, abrir a segunda.", VAbrir, 2},
		{"Rodar a quatro, obrigada.", VRodar, 4},
		{"Enviar o cinco pra revisão", VRevisao, 5},
	}
	for _, c := range casos {
		e := EscolherDaLista(c.frase, "Colmeia", 5, false)
		if !e.Certa || e.Verbo != c.verbo || e.Numero != c.numero {
			t.Errorf("%q: veio %+v, esperava %v %d", c.frase, e, c.verbo, c.numero)
		}
	}
}

func TestGramaticaSemConfiancaPerguntaDeNovo(t *testing.T) {
	for _, frase := range []string{"", "[Música]", "Qual a previsão do tempo?", "Rodar a loja web.", "abrir o login com google", "hmm", "obrigada"} {
		if e := EscolherDaLista(frase, "Colmeia", 3, false); e.Certa {
			t.Errorf("%q: entendeu %+v", frase, e)
		}
	}
	// Um número que a página não tem: não vira o mais parecido, e a abelha diz até onde vai.
	e := EscolherDaLista("abrir a sete", "Colmeia", 3, false)
	if e.Certa || e.ForaDaLista != 7 {
		t.Errorf("fora: %+v", e)
	}
	// Com a página de dois, "abrir a três" não vale.
	if e := EscolherDaLista("abrir a três", "Colmeia", 2, false); e.Certa {
		t.Errorf("três com dois itens: %+v", e)
	}
	// Sem lista nenhuma, só os pedidos sem número.
	if e := EscolherDaLista("abrir a um", "Colmeia", 0, false); e.Certa {
		t.Errorf("vazia: %+v", e)
	}
	if e := EscolherDaLista("fechar", "Colmeia", 0, false); !e.Certa || e.Verbo != VFechar {
		t.Errorf("fechar na vazia: %+v", e)
	}
}

func TestGramaticaComPerguntaNoAr(t *testing.T) {
	if e := EscolherDaLista("Sim.", "Colmeia", 3, true); !e.Certa || e.Verbo != VSim {
		t.Errorf("sim: %+v", e)
	}
	if e := EscolherDaLista("Não.", "Colmeia", 3, true); !e.Certa || e.Verbo != VNao {
		t.Errorf("não: %+v", e)
	}
	// Como o Whisper escreveu a voz sintética: "Sim, pode." e "Pode.".
	for _, f := range []string{"Sem podi.", "Podi."} {
		if e := EscolherDaLista(f, "Colmeia", 3, true); !e.Certa || e.Verbo != VSim {
			t.Errorf("%q: %+v", f, e)
		}
	}
	if e := EscolherDaLista("Pode sim.", "Colmeia", 3, true); !e.Certa || e.Verbo != VSim {
		t.Errorf("pode sim: %+v", e)
	}
	// Sem pergunta, "sim" não é da lista.
	if e := EscolherDaLista("Sim.", "Colmeia", 3, false); e.Certa {
		t.Errorf("sim sem pergunta: %+v", e)
	}
}

func TestPareceNumero(t *testing.T) {
	for _, s := range []string{"um", "hum", "1", "segunda", "tres", "a primeira"} {
		if !PareceNumero(s) {
			t.Errorf("%q", s)
		}
	}
	for _, s := range []string{"login com google", "loja web", "cache", ""} {
		if PareceNumero(s) {
			t.Errorf("%q", s)
		}
	}
}

func TestAcaoDaLista(t *testing.T) {
	q := quadro()
	itens := MontarLista(q)
	// 1. Login com Google (o Claude Code pede aprovação), 2. Ajustar o cache, 3. Testes de integração (erro).
	if len(itens) != 3 || itens[0].TarefaID != 101 || itens[1].TarefaID != 102 || itens[2].TarefaID != 104 {
		t.Fatalf("%+v", itens)
	}
	if r := AcaoDaLista(VAbrir, itens[0], q); r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoAbrir || r.Acao.TarefaID != 101 {
		t.Errorf("abrir: %+v", r)
	}
	// Responder: o agente que espera, não o que trabalha.
	if r := AcaoDaLista(VResponder, itens[0], q); r.Acao == nil || r.Acao.Tipo != AcaoResponder || r.Acao.AgenteID != 1 || r.Confirmar {
		t.Errorf("responder: %+v", r)
	}
	// Sem agente rodando, diz e escuta de novo.
	if r := AcaoDaLista(VResponder, itens[1], q); r.Acao != nil || !r.Ouvir || !strings.Contains(r.Texto, "Nenhum agente") {
		t.Errorf("responder sem agente: %+v", r)
	}
	if r := AcaoDaLista(VConcluir, itens[1], q); !r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoEncerrar {
		t.Errorf("concluir pede o sim: %+v", r)
	}
	if r := AcaoDaLista(VRevisao, itens[0], q); r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoRevisao {
		t.Errorf("revisão: %+v", r)
	}
	if r := AcaoDaLista(VRevisao, itens[2], q); r.Acao != nil || !strings.Contains(r.Texto, "já está em Revisão") {
		t.Errorf("já em revisão: %+v", r)
	}
	// Rodar e parar valem para o projeto da tarefa.
	if r := AcaoDaLista(VRodar, itens[0], q); r.Confirmar || r.Acao == nil || r.Acao.Tipo != AcaoRodar || r.Acao.ExecucaoID != 11 {
		t.Errorf("rodar: %+v", r)
	}
	if r := AcaoDaLista(VParar, itens[0], q); r.Acao != nil || !strings.Contains(r.Texto, "Nada do projeto loja-web") {
		t.Errorf("parar parado: %+v", r)
	}
	if r := AcaoDaLista(VParar, itens[2], q); !r.Confirmar || r.Acao == nil || r.Acao.ExecucaoID != 21 {
		t.Errorf("parar pede o sim: %+v", r)
	}
	if r := AcaoDaLista(VRodar, itens[2], q); r.Acao != nil || !r.Ouvir || !strings.Contains(r.Texto, "duas configurações") {
		t.Errorf("rodar com duas: %+v", r)
	}
	// A tarefa sumiu depois da lista.
	q.Tarefas = q.Tarefas[1:]
	if r := AcaoDaLista(VAbrir, itens[0], q); r.Acao != nil || !strings.Contains(r.Texto, "já não existe") {
		t.Errorf("sumiu: %+v", r)
	}
}
