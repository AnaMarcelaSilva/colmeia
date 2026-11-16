package montagem

import (
	"strings"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/formato"
)

func entradaDeExemplo() Entrada {
	return Entrada{TarefaID: 7, Titulo: "Integrar o painel com a gestão web das lojas", Projeto: "loja-web", Estado: "Em revisão",
		Nota:    formato.ExemploNotaDaily,
		Imagens: []Imagem{{ID: 41, Nome: "painel-gestao.png", Autor: "agente", Largura: 1600, Altura: 900}, {ID: 40, Nome: "instalador.png", Legenda: "O campo novo", Autor: "voce", Largura: 800, Altura: 1200}},
		Feito:   []string{"Claude Code trabalhou 40 min", "Capturou 2 telas"}}
}

func TestSimplesSaiDaNota(t *testing.T) {
	m := Simples(entradaDeExemplo())
	if m.Origem != OrigemSimples || m.Base == "" {
		t.Fatalf("origem/base: %+v", m)
	}
	if m.Topico != "Integrar o painel" {
		t.Errorf("tópico: %q", m.Topico)
	}
	if !strings.HasPrefix(m.Frase, "A gestão web de cada loja") {
		t.Errorf("frase: %q", m.Frase)
	}
	titulos := []string{}
	for _, c := range m.Cartoes {
		titulos = append(titulos, c.Titulo)
	}
	// Resumo, o fluxo numerado num cartão só, o item solto, as duas imagens e o Falta por último.
	quer := []string{"O que mudou", "", "Como funciona", "Como ficou", "Como ficou", "Falta"}
	if strings.Join(titulos, "|") != strings.Join(quer, "|") {
		t.Fatalf("cartões: %v", titulos)
	}
	if !strings.HasPrefix(m.Cartoes[2].Texto, "1. Campo novo") || !strings.Contains(m.Cartoes[2].Texto, "\n4. No painel") {
		t.Errorf("fluxo: %q", m.Cartoes[2].Texto)
	}
	if m.Cartoes[3].Imagem != 41 || m.Cartoes[3].Legenda != "painel-gestao.png" || m.Cartoes[4].Legenda != "O campo novo" {
		t.Errorf("imagens: %+v %+v", m.Cartoes[3], m.Cartoes[4])
	}
	if !strings.HasPrefix(m.Cartoes[5].Texto, "Falta decidir") {
		t.Errorf("falta: %q", m.Cartoes[5].Texto)
	}
}

func TestSimplesSemNotaNuncaFicaVazia(t *testing.T) {
	e := Entrada{TarefaID: 3, Titulo: "Corrigir o código de barras da balança", Estado: "Esperando você", Feito: []string{"Claude Code trabalhou 12 min", "Está esperando sua resposta."}}
	m := Simples(e)
	if m.Topico != "Corrigir o código de barras" || m.Frase != "Em andamento, esperando uma resposta." {
		t.Errorf("%q %q", m.Topico, m.Frase)
	}
	if len(m.Cartoes) != 1 || !strings.Contains(m.Cartoes[0].Texto, "trabalhou 12 min") {
		t.Errorf("cartões: %+v", m.Cartoes)
	}
	// Sem nada, ainda assim um cartão.
	m = Simples(Entrada{TarefaID: 4, Titulo: "Nova", Estado: "Concluída"})
	if len(m.Cartoes) != 1 || m.Cartoes[0].Texto != "Concluída." {
		t.Errorf("vazia: %+v", m.Cartoes)
	}
}

func TestSimplesCabeEmSeisCartoes(t *testing.T) {
	nota := "Frase.\n- Um, a.\n- Dois, b.\n- Três, c.\n- Quatro, d.\n- Cinco, e.\n- Seis, f.\n- Falta g."
	m := Simples(Entrada{TarefaID: 1, Titulo: "T", Nota: nota, Imagens: []Imagem{{ID: 9}}})
	if len(m.Cartoes) != MaxCartoes {
		t.Fatalf("%d cartões", len(m.Cartoes))
	}
	if m.Cartoes[0].Titulo != "O que mudou" || m.Cartoes[1].Titulo != "Um" || m.Cartoes[4].Imagem != 9 || m.Cartoes[5].Titulo != "Falta" {
		t.Errorf("%+v", m.Cartoes)
	}
}

func TestValidarEEstrito(t *testing.T) {
	e := entradaDeExemplo()
	bom := `{"topico":"Endereço da loja no painel","frase":"O painel abre a gestão de cada loja pelo endereço certo.","cartoes":[{"titulo":"Endereço da loja","texto":"O painel passa a abrir a gestão web pelo endereço da loja."},{"titulo":"Como ficou","imagem":41,"legenda":"Painel com a loja escolhida"},{"titulo":"Falta","texto":"Decidir com o time onde fica a gestão web."}]}`
	m, err := Validar("```json\n"+bom+"\n```", e)
	if err != nil {
		t.Fatal(err)
	}
	if m.Origem != OrigemMontador || m.TarefaID != 7 || len(m.Cartoes) != 3 || m.Cartoes[1].Imagem != 41 || m.Base != Base(e) {
		t.Errorf("%+v", m)
	}
	for nome, texto := range map[string]string{
		"campo a mais":       `{"topico":"a","frase":"b","cartoes":[{"titulo":"c","texto":"d"}],"x":1}`,
		"texto depois":       bom + " ok",
		"sem cartões":        `{"topico":"a","frase":"b","cartoes":[]}`,
		"imagem de outro":    `{"topico":"a","frase":"b","cartoes":[{"titulo":"c","imagem":99}]}`,
		"imagem com texto":   `{"topico":"a","frase":"b","cartoes":[{"titulo":"c","imagem":41,"texto":"d"}]}`,
		"cartão vazio":       `{"topico":"a","frase":"b","cartoes":[{"titulo":"c"}]}`,
		"tópico longo":       `{"topico":"` + strings.Repeat("a", 61) + `","frase":"b","cartoes":[{"titulo":"c","texto":"d"}]}`,
		"três imagens":       `{"topico":"a","frase":"b","cartoes":[{"titulo":"c","imagem":41},{"titulo":"c","imagem":40},{"titulo":"c","imagem":41}]}`,
		"segredo":            `{"topico":"a","frase":"senha: hunter2hunter2","cartoes":[{"titulo":"c","texto":"d"}]}`,
		"controle":           "{\"topico\":\"a\",\"frase\":\"b\",\"cartoes\":[{\"titulo\":\"c\",\"texto\":\"d\x07\"}]}",
		"tópico em 2 linhas": `{"topico":"a\nb","frase":"b","cartoes":[{"titulo":"c","texto":"d"}]}`,
	} {
		if _, err := Validar(texto, e); err == nil {
			t.Errorf("%s: aceitou", nome)
		}
	}
}

func TestInstrucoesLevamOEstiloESoOsDados(t *testing.T) {
	e := entradaDeExemplo()
	b, err := Instrucoes(e)
	if err != nil {
		t.Fatal(err)
	}
	s := string(b)
	for _, trecho := range []string{formato.Estilo[:40], "<tarefa>", `"nota_da_daily"`, `"imagens"`, "SOMENTE com um JSON", "Falta"} {
		if !strings.Contains(s, trecho) {
			t.Errorf("faltou %q", trecho)
		}
	}
	if strings.Contains(s, "largura") {
		t.Error("o tamanho da imagem não vai ao agente")
	}
}

func TestDesenharColunasComSetas(t *testing.T) {
	a := Montagem{TarefaID: 1, Topico: "Trava da comanda", Frase: "f", Cartoes: []Cartao{{Titulo: "Trava", Texto: "x"}, {Titulo: "Falta", Texto: "y"}}}
	b := Montagem{TarefaID: 2, Topico: "Código de barras", Frase: "f", Cartoes: []Cartao{{Titulo: "Código", Texto: "z"}, {Titulo: "Como ficou", Imagem: 5, Legenda: "Tela"}}}
	c := Montagem{TarefaID: 3, Topico: "Outro", Frase: "f", Cartoes: []Cartao{{Titulo: "O", Texto: "w"}}}
	pecas := Desenhar([]Coluna{
		{Montagem: a, Secao: "loja-web", SecaoID: 10},
		{Montagem: b, Secao: "loja-web", SecaoID: 10, Imagens: map[int64]Imagem{5: {ID: 5, Largura: 1920, Altura: 1080}}},
		{Montagem: c, Secao: "api-pedidos", SecaoID: 11},
	})
	por := map[string]Peca{}
	for _, p := range pecas {
		por[p.Ref] = p
	}
	// Três cartões de tarefa lado a lado, no mesmo y.
	if por["t1"].Tipo != "tarefa" || por["t2"].X != MargemEsq+LarguraColuna+EntreColunas || por["t3"].Y != por["t1"].Y {
		t.Errorf("tarefas: %+v %+v %+v", por["t1"], por["t2"], por["t3"])
	}
	// Os cartões descem, com setas, e o último liga à tarefa seguinte.
	if por["t1c0"].Y <= por["t1"].Y || por["t1c1"].Y <= por["t1c0"].Y || por["t1c1"].Cor != "rosa" || por["t1c0"].Cor != "azul" {
		t.Errorf("coluna 1: %+v %+v", por["t1c0"], por["t1c1"])
	}
	for _, lig := range []struct{ de, para string }{{"t1", "t1c0"}, {"t1c0", "t1c1"}, {"t1c1", "t2"}, {"t2c1", "t3"}} {
		if p, ok := por[lig.para+"-de-"+lig.de]; !ok || p.Tipo != "ligacao" || p.De != lig.de || p.Para != lig.para {
			t.Errorf("faltou a ligação %s → %s", lig.de, lig.para)
		}
	}
	// A imagem mantém a proporção e leva a legenda no título.
	if img := por["t2c1"]; img.Tipo != "imagem" || img.AnexoID != 5 || img.Titulo != "Tela" || img.Altura < LarguraColuna*9/16 {
		t.Errorf("imagem: %+v", img)
	}
	// Dois rótulos de seção, cobrindo as colunas delas, acima dos cartões.
	if s := por["s10"]; s.Tipo != "texto" || s.Texto != "loja-web" || s.Largura != 2*LarguraColuna+EntreColunas || s.Y >= por["t1"].Y {
		t.Errorf("seção: %+v", s)
	}
	if s := por["s11"]; s.Texto != "api-pedidos" || s.X != por["t3"].X || s.Largura != LarguraColuna {
		t.Errorf("seção 2: %+v", s)
	}
	// Cada ligação vem depois das pontas dela.
	visto := map[string]bool{}
	for _, p := range pecas {
		if p.Tipo == "ligacao" && (!visto[p.De] || !visto[p.Para]) {
			t.Errorf("ligação %s antes das pontas", p.Ref)
		}
		visto[p.Ref] = true
	}
}
