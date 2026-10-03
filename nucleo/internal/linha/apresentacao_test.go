package linha

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

func titulos(d Deck) []string {
	var t []string
	for _, s := range d.Slides {
		t = append(t, s.Titulo)
	}
	return t
}

func TestApresentacaoDaDaily(t *testing.T) {
	h := &historico{t: t, eventos: semana(t)}
	// Uma tarefa só criada hoje não vira slide: entra na capa como nova.
	h.add("2026-09-28 08:50", "tarefa.criada", dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 3}, map[string]any{"id": 3, "titulo": "Relatório lento", "projeto_id": 1})
	c := contexto()
	c.Tarefas[3] = TarefaAtual{ID: 3, Titulo: "Relatório lento", Coluna: "backlog", ProjetoID: 1}
	d := Apresentacao(h.eventos, em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")

	if d.Titulo != "Daily · segunda, 28 de setembro" || d.Periodo != "desde sexta, 25 de setembro · loja-web" || d.ChaveNota != "2026-09-28" {
		t.Errorf("cabeçalho: %q / %q / %q", d.Titulo, d.Periodo, d.ChaveNota)
	}
	// Concluídas primeiro; a tarefa que aparece na sexta e hoje é um slide só.
	if got := strings.Join(titulos(d), " | "); got != "Corrigir desconto | Nova tela de pedidos" {
		t.Fatalf("slides: %s", got)
	}
	desconto, tela := d.Slides[0], d.Slides[1]
	if desconto.Grupo != GrupoConcluidas || desconto.Status != "Concluído" || strings.Join(desconto.Partes, ",") != "Sexta" {
		t.Errorf("slide do desconto: %+v", desconto)
	}
	// A captura não vira tópico: aparece como mídia e no número de capturas.
	esperado := []string{"Concluída.", "Codex (testador) parou com erro (código 1)."}
	if len(desconto.Feito) != 1 || strings.Join(desconto.Feito[0].Itens, "|") != strings.Join(esperado, "|") {
		t.Errorf("o que foi feito no desconto: %+v", desconto.Feito)
	}
	if desconto.Numeros.Erros != 1 || desconto.Numeros.Sessoes != 1 || desconto.Numeros.Capturas != 1 || desconto.Numeros.TempoS != 300 {
		t.Errorf("números do desconto: %+v", desconto.Numeros)
	}
	if tela.Grupo != GrupoAguardando || strings.Join(tela.Partes, ",") != "Sexta,Hoje" || len(tela.Feito) != 2 {
		t.Fatalf("slide da tela: %+v", tela)
	}
	if tela.Feito[0].Itens[0] != "Claude Code (dev) trabalhou 1h05 e esperou você 12 min." || tela.Feito[0].Itens[1] != "Foi para Revisão." {
		t.Errorf("sexta da tela: %+v", tela.Feito[0])
	}
	if tela.Feito[1].Parte != "Hoje" || tela.Feito[1].Itens[0] != "Claude Code (revisor) trabalhando agora desde 08:30." {
		t.Errorf("hoje da tela: %+v", tela.Feito[1])
	}
	capa := d.Capa
	if capa.Numeros.Concluidas != 1 || capa.Numeros.Aguardando != 1 || capa.Numeros.Erros != 1 || capa.Numeros.Novas != 1 ||
		capa.Numeros.TempoS != 4200 || len(capa.Numeros.PorFerramenta) != 2 {
		t.Errorf("números da capa: %+v", capa.Numeros)
	}
	if len(capa.Partes) != 2 || capa.Partes[0].Titulo != "Sexta (25/09)" || capa.Partes[1].Titulo != "Hoje" ||
		fmt.Sprint(capa.Partes[0].Tarefas) != "[2 1]" || fmt.Sprint(capa.Partes[1].Tarefas) != "[1]" {
		t.Errorf("partes da capa: %+v", capa.Partes)
	}
	if strings.Join(capa.Novas, ",") != "Relatório lento" {
		t.Errorf("tarefas novas: %v", capa.Novas)
	}
	if len(capa.Destaques) != 3 || capa.Destaques[0] != "Concluí “Corrigir desconto”" || capa.Destaques[1] != "Esperando minha resposta: “Nova tela de pedidos”" {
		t.Errorf("destaques: %q", capa.Destaques)
	}
	// Listas sempre como lista, nunca null.
	bruto, _ := json.Marshal(Apresentacao(nil, em("2026-09-28 00:00"), em("2026-09-28 00:00"), Contexto{Agora: c.Agora, Fuso: fuso}, "daily"))
	for _, campo := range []string{`"slides":[]`, `"partes":[`, `"destaques":[]`, `"novas":[]`, `"vazio":true`} {
		if !strings.Contains(string(bruto), campo) {
			t.Errorf("deck vazio sem %s: %s", campo, bruto)
		}
	}
}

func TestApresentacaoDaSprint(t *testing.T) {
	d := Apresentacao(semana(t), em("2026-09-21 00:00"), em("2026-09-27 00:00"), contexto(), "sprint")
	if d.Titulo != "Sprint · 21/09 a 27/09" || d.Periodo != "7 dias · loja-web" || d.ChaveNota != "2026-09-21..2026-09-27" || d.De != "2026-09-21" || d.Ate != "2026-09-27" {
		t.Errorf("cabeçalho: %+v", d)
	}
	if got := strings.Join(titulos(d), " | "); got != "Corrigir desconto | Nova tela de pedidos" {
		t.Fatalf("slides: %s", got)
	}
	// A coluna é a do fim do período, não a de agora.
	if tela := d.Slides[1]; tela.Coluna != "revisao" || tela.Grupo != GrupoRevisao || tela.Secao != "loja-web" {
		t.Errorf("tela na sprint: %+v", tela)
	}
	if !strings.HasPrefix(d.Slides[0].Feito[0].Itens[0], "25/09 · ") {
		t.Errorf("tópicos da sprint sem data: %v", d.Slides[0].Feito)
	}
	if len(d.Capa.Partes) != 1 || d.Capa.Partes[0].Titulo != "loja-web" || d.Capa.Numeros.Revisao != 1 {
		t.Errorf("capa da sprint: %+v", d.Capa)
	}
	// O ano só aparece quando o período cruza a virada do ano.
	virada := Apresentacao(nil, em("2026-12-28 00:00"), em("2027-01-03 00:00"), contexto(), "sprint")
	if virada.Titulo != "Sprint · 28/12/2026 a 03/01/2027" || !strings.HasPrefix(virada.Periodo, "7 dias") {
		t.Errorf("sprint na virada do ano: %q / %q", virada.Titulo, virada.Periodo)
	}
}

func TestApresentacaoLimitaOsSlides(t *testing.T) {
	h := &historico{t: t}
	c := contexto()
	c.Tarefas = map[int64]TarefaAtual{}
	c.Ativos = nil
	for i := int64(1); i <= MaxSlides+5; i++ {
		e := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: i, Agente: 100 + i}
		titulo := fmt.Sprintf("Tarefa %d", i)
		c.Tarefas[i] = TarefaAtual{ID: i, Titulo: titulo, Coluna: "revisao", ProjetoID: 1}
		h.add("2026-09-28 08:00", "agente.terminou", e, map[string]any{"ferramenta": "claude", "papel": "dev", "titulo": titulo, "motivo": "terminou", "trabalhando_s": 30})
		h.add("2026-09-28 08:01", "agente.terminou", e, map[string]any{"ferramenta": "claude", "papel": "dev", "titulo": titulo, "motivo": "terminou", "trabalhando_s": 20})
		h.add("2026-09-28 08:02", "agente.terminou", e, map[string]any{"ferramenta": "claude", "papel": "dev", "titulo": titulo, "motivo": "terminou", "trabalhando_s": 10})
	}
	d := Apresentacao(h.eventos, em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")
	if len(d.Slides) != MaxSlides || d.Mais != 5 || d.Capa.Numeros.Revisao != MaxSlides+5 {
		t.Errorf("%d slides, mais %d, em revisão %d", len(d.Slides), d.Mais, d.Capa.Numeros.Revisao)
	}
	// As sessões do mesmo agente viram um tópico só, com o tempo somado.
	if itens := d.Slides[0].Feito[0].Itens; len(itens) != 1 || itens[0] != "Claude Code (dev) trabalhou 1 min em 3 sessões." {
		t.Errorf("sessões juntas: %q", itens)
	}
}

func TestTopicosJuntamRepeticoesSemSequencia(t *testing.T) {
	h := &historico{t: t}
	c := contexto()
	c.Ativos = nil
	e := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 10}
	outro := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 11}
	sessao := func(quando string, esc dados.Escopo, papel string, s, aguardou int) {
		h.add(quando, "agente.terminou", esc, map[string]any{"ferramenta": "claude", "papel": papel, "titulo": "Nova tela de pedidos", "motivo": "terminou", "trabalhando_s": s, "aguardando_s": aguardou})
	}
	sessao("2026-09-28 08:00", e, "dev", 600, 0)
	h.add("2026-09-28 08:05", "anexo.adicionado", e, map[string]any{"anexo": 7, "origem": "arquivo", "titulo": "Nova tela de pedidos"})
	sessao("2026-09-28 08:10", outro, "revisor", 120, 0)
	h.add("2026-09-28 08:20", "anexo.adicionado", e, map[string]any{"anexo": 8, "origem": "arquivo", "tipo": "video", "titulo": "Nova tela de pedidos"})
	sessao("2026-09-28 09:00", e, "dev", 3600, 300)
	h.add("2026-09-28 09:30", "tarefa.atualizada", e, map[string]any{"tarefa": map[string]any{"id": 1, "titulo": "Nova tela de pedidos", "coluna": "revisao"}, "coluna_antes": "trabalhando"})
	d := Apresentacao(h.eventos, em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")
	if len(d.Slides) != 1 || len(d.Slides[0].Feito) != 1 {
		t.Fatalf("slides: %+v", d.Slides)
	}
	esperado := []string{
		"Claude Code (dev) trabalhou 1h10 em 2 sessões e esperou você 5 min.",
		"Claude Code (revisor) trabalhou 2 min.",
		"Foi para Revisão.",
	}
	if got := d.Slides[0].Feito[0].Itens; strings.Join(got, "|") != strings.Join(esperado, "|") {
		t.Errorf("tópicos:\n%q\nesperado:\n%q", got, esperado)
	}
	if n := d.Slides[0].Numeros; n.Sessoes != 3 || n.Capturas != 1 {
		t.Errorf("números: %+v", n)
	}
}

func TestDestaqueComTituloLongo(t *testing.T) {
	longo := "Migrar os filtros da listagem para a API nova com paginação, ordenação por várias colunas e cache"
	got := citar([]string{longo, "Relatório de vendas lento", "A", "B", "C"})
	if got != "“Migrar os filtros da listagem para a API…”, “Relatório de vendas lento”, “A” e mais 2" {
		t.Errorf("citar: %q", got)
	}
	if encurtar("paginação, ordenação e cache", 11) != "paginação…" {
		t.Errorf("corte com vírgula: %q", encurtar("paginação, ordenação e cache", 11))
	}
}

func TestCompletarComAnexosENotas(t *testing.T) {
	d := Apresentacao(semana(t), em("2026-09-21 00:00"), em("2026-09-27 00:00"), contexto(), "sprint")
	anexos := []dados.Anexo{
		{ID: 3, TarefaID: 2, Tipo: "imagem", Largura: 800, Altura: 600},
		{ID: 4, TarefaID: 2, Tipo: "imagem", Removido: true},
		{ID: 5, TarefaID: 2, Tipo: "video", Nome: "demo.mp4", Bytes: 1234},
		{ID: 6, TarefaID: 1, Tipo: "imagem"},
	}
	notas := map[int64]dados.Nota{2: {TarefaID: 2, Texto: "Mostrar o desconto aplicado"}}
	anteriores := map[int64]dados.Nota{1: {TarefaID: 1, Periodo: "2026-09-14..2026-09-20", Texto: "Falta o filtro"}}
	d.Completar(anexos, notas, anteriores)
	desconto, tela := d.Slides[0], d.Slides[1]
	if len(desconto.Anexos) != 2 || desconto.Anexos[0].ID != 5 || desconto.Anexos[1].ID != 3 || desconto.Numeros.Capturas != 1 {
		t.Errorf("anexos do desconto (removido não entra, mais novo primeiro): %+v", desconto.Anexos)
	}
	if desconto.Nota != "Mostrar o desconto aplicado" || desconto.NotaAnterior != nil {
		t.Errorf("nota do desconto: %+v", desconto)
	}
	if tela.Nota != "" || tela.NotaAnterior == nil || tela.NotaAnterior.Periodo != "14/09 a 20/09" {
		t.Errorf("nota anterior da tela: %+v", tela.NotaAnterior)
	}
}

func TestLinhaComVideoENota(t *testing.T) {
	h := &historico{t: t}
	t1 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1}
	h.add("2026-09-28 08:00", "anexo.adicionado", t1, map[string]any{"anexo": 7, "origem": "arquivo", "tipo": "video", "titulo": "Nova tela de pedidos"})
	h.add("2026-09-28 08:01", "anexo.adicionado", t1, map[string]any{"anexo": 8, "origem": "arquivo", "titulo": "Nova tela de pedidos"})
	h.add("2026-09-28 08:02", "nota.atualizada", t1, map[string]any{"tipo": "daily", "tamanho": 10, "titulo": "Nova tela de pedidos"})
	h.add("2026-09-28 08:03", "nota.atualizada", t1, map[string]any{"tipo": "daily", "tamanho": 12, "titulo": "Nova tela de pedidos"})
	dias := Montar(h.eventos, contexto())
	itens := dias[0].Itens
	if len(itens) != 3 {
		t.Fatalf("itens: %+v", itens)
	}
	if itens[0].Tipo != TipoNota || itens[0].Texto != "Anotou na daily sobre “Nova tela de pedidos”." || itens[0].Curto != "Anotou na daily." {
		t.Errorf("nota: %+v", itens[0])
	}
	if itens[1].Texto != "Imagem anexada em “Nova tela de pedidos”." || len(itens[1].Videos) != 0 {
		t.Errorf("imagem: %+v", itens[1])
	}
	if itens[2].Texto != "Vídeo anexado em “Nova tela de pedidos”." || fmt.Sprint(itens[2].Videos) != "[7]" || itens[2].Curto != "Vídeo anexado." {
		t.Errorf("vídeo: %+v", itens[2])
	}
	// A sprint não leva o vídeo para as capturas (ela salva PNGs).
	r := Sprint(h.eventos, em("2026-09-28 00:00"), em("2026-09-28 00:00"), contexto())
	if len(r.Capturas) != 1 || r.Capturas[0].Anexo != 8 {
		t.Errorf("capturas da sprint: %+v", r.Capturas)
	}
}
