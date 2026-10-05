package linha

import (
	"encoding/json"
	"regexp"
	"strings"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// Sem o tempo dos agentes, nada de duração nem de espera em lugar nenhum:
// nem nos números (as chaves somem do JSON), nem nos textos e no Markdown.
func TestSemTempoDosAgentes(t *testing.T) {
	c := contexto()
	c.MostrarTempo = false
	eventos := semana(t)
	proibidos := []string{"trabalhou", "esperou", "trabalharam", "Tempo de agentes", "Agentes", "agentes 1h", "1h05", "12 min"}
	hora := regexp.MustCompile(`desde \d\d:\d\d`)
	conferir := func(onde string, v any) string {
		t.Helper()
		bruto, _ := json.Marshal(v)
		texto := string(bruto)
		for _, chave := range []string{`"tempo_s"`, `"tempo_total_s"`, `"por_ferramenta"`, `"tempo_agentes":true`} {
			if strings.Contains(texto, chave) {
				t.Errorf("%s com %s: %s", onde, chave, texto)
			}
		}
		for _, p := range proibidos {
			if strings.Contains(texto, p) {
				t.Errorf("%s com %q: %s", onde, p, texto)
			}
		}
		if hora.MatchString(texto) {
			t.Errorf("%s com a hora de desde: %s", onde, texto)
		}
		return texto
	}

	dias := Montar(eventos, c)
	conferir("linha do tempo", dias)
	if dias[0].Itens[0].Texto != "Claude Code (revisor) trabalhando agora em “Nova tela de pedidos”." {
		t.Errorf("sessão aberta: %q", dias[0].Itens[0].Texto)
	}
	if got := dias[1].Itens[len(dias[1].Itens)-1].Texto; got != "Claude Code (dev) terminou uma sessão em “Nova tela de pedidos”." {
		t.Errorf("sessão terminada: %q", got)
	}
	if dias[1].Resumo != "1 concluída" {
		t.Errorf("resumo do dia: %q", dias[1].Resumo)
	}

	d := Daily(eventos, c)
	conferir("daily", d)
	if !strings.HasPrefix(d.Texto, "Na sexta (25/09): concluí Corrigir desconto; avancei Nova tela de pedidos; tive erro em Corrigir desconto.\n") {
		t.Errorf("texto da daily: %q", d.Texto)
	}

	sprint := Sprint(eventos, em("2026-09-21 00:00"), em("2026-09-27 00:00"), c)
	conferir("sprint", sprint)
	conferir("markdown da sprint", sprint.Markdown)

	deck := Apresentacao(eventos, em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")
	conferir("deck da daily", deck)
	tela := deck.Slides[1]
	// A sessão continua contando; só a duração some.
	if tela.Numeros.Sessoes != 2 || tela.Feito[0].Itens[0] != "Claude Code (dev) terminou uma sessão." || tela.Feito[1].Itens[0] != "Claude Code (revisor) trabalhando agora." {
		t.Errorf("slide sem tempo: %+v / %+v", tela.Numeros, tela.Feito)
	}
	for _, dd := range deck.Capa.Destaques {
		if strings.Contains(dd, "gentes") {
			t.Errorf("destaque com tempo: %q", dd)
		}
	}
	deckSprint := Apresentacao(eventos, em("2026-09-21 00:00"), em("2026-09-27 00:00"), c, "sprint")
	conferir("deck da sprint", deckSprint)

	// Ligado, os mesmos eventos mostram o tempo (os dados continuam gravados).
	c.MostrarTempo = true
	if d := Daily(eventos, c); !d.TempoAgentes || !strings.Contains(d.Texto, "agentes trabalharam 1h10") {
		t.Errorf("com o tempo ligado: %+v", d)
	}
}

// Sessões de um agente sem o tempo: "Claude Code (dev): 3 sessões".
func TestJuntarSemTempo(t *testing.T) {
	s := Slide{brutos: []bruto{
		{parte: "Hoje", agente: "Claude Code (dev)"},
		{parte: "Hoje", agente: "Claude Code (dev)"},
		{parte: "Hoje", agente: "Claude Code (dev)"},
		{parte: "Hoje", agente: "Codex (testador)"},
	}}
	feito := s.juntar(false)
	if len(feito) != 1 || strings.Join(feito[0].Itens, "|") != "Claude Code (dev): 3 sessões.|Codex (testador) terminou uma sessão." {
		t.Errorf("juntar sem tempo: %+v", feito)
	}
}

// Dois workspaces e três projetos, um nome repetido: estudos tem loja-web e
// cliente-x; trabalho-x tem outro loja-web. A ordem é a da barra lateral.
func geral(t *testing.T) ([]dados.Evento, Contexto) {
	h := &historico{t: t}
	tarefaEm := func(id, projeto int64, titulo, coluna string) map[string]any {
		return map[string]any{"id": id, "titulo": titulo, "coluna": coluna, "projeto_id": projeto}
	}
	esc := func(projeto, tarefa int64) dados.Escopo {
		return dados.Escopo{Perfil: 1, Projeto: projeto, Tarefa: tarefa}
	}
	h.add("2026-09-21 09:00", "tarefa.criada", esc(1, 1), map[string]any{"id": 1, "titulo": "Corrigir frete", "projeto_id": 1})
	h.add("2026-09-21 09:01", "tarefa.criada", esc(2, 2), map[string]any{"id": 2, "titulo": "Importar planilha", "projeto_id": 2})
	h.add("2026-09-21 09:02", "tarefa.criada", esc(3, 3), map[string]any{"id": 3, "titulo": "Tela de cupons", "projeto_id": 3})
	h.add("2026-09-25 10:00", "tarefa.atualizada", esc(3, 3), map[string]any{"tarefa": tarefaEm(3, 3, "Tela de cupons", "concluido"), "coluna_antes": "backlog", "origem": "voce"})
	h.add("2026-09-25 11:00", "tarefa.atualizada", esc(1, 1), map[string]any{"tarefa": tarefaEm(1, 1, "Corrigir frete", "concluido"), "coluna_antes": "backlog", "origem": "voce"})
	h.add("2026-09-25 12:00", "tarefa.atualizada", esc(2, 2), map[string]any{"tarefa": tarefaEm(2, 2, "Importar planilha", "revisao"), "coluna_antes": "backlog", "origem": "voce"})
	c := Contexto{
		Agora: em("2026-09-28 09:00"), Fuso: fuso,
		Projetos:   map[int64]string{1: "loja-web", 2: "cliente-x", 3: "loja-web"},
		Workspaces: map[int64]string{1: "estudos", 2: "estudos", 3: "trabalho-x"},
		// Na lateral: estudos (cliente-x, loja-web), depois trabalho-x (loja-web).
		Ordem: map[int64]int{2: 0, 1: 1, 3: 2},
		Tarefas: map[int64]TarefaAtual{
			1: {ID: 1, Titulo: "Corrigir frete", Coluna: "concluido", ProjetoID: 1},
			2: {ID: 2, Titulo: "Importar planilha", Coluna: "revisao", ProjetoID: 2},
			3: {ID: 3, Titulo: "Tela de cupons", Coluna: "concluido", ProjetoID: 3},
		},
		VariosProjetos: true, VariosWorkspaces: true,
		Escopo: "todos os projetos · 3 projetos",
	}
	return h.eventos, c
}

func TestDailyGeralPorProjeto(t *testing.T) {
	eventos, c := geral(t)
	d := Apresentacao(eventos, em("2026-09-28 00:00"), em("2026-09-28 00:00"), c, "daily")
	var secoes []string
	for _, s := range d.Slides {
		secoes = append(secoes, s.Secao+"#"+s.Workspace)
	}
	// A ordem da lateral, e os dois loja-web separados pelo workspace.
	if got := strings.Join(secoes, " | "); got != "estudos · cliente-x#estudos | estudos · loja-web#estudos | trabalho-x · loja-web#trabalho-x" {
		t.Errorf("seções da daily: %s", got)
	}
	if d.Slides[1].SecaoID != 1 || d.Slides[2].SecaoID != 3 || d.Slides[2].Projeto != "loja-web" {
		t.Errorf("ids das seções: %+v", d.Slides)
	}
	// Com mais de um projeto, a capa da daily tem uma coluna por projeto.
	if len(d.Capa.Partes) != 3 || d.Capa.Partes[2].Titulo != "trabalho-x · loja-web" {
		t.Errorf("partes da capa: %+v", d.Capa.Partes)
	}
	if !strings.HasSuffix(d.Periodo, " · todos os projetos · 3 projetos") {
		t.Errorf("período: %q", d.Periodo)
	}

	texto := Daily(eventos, c).Texto
	esperado := "estudos · cliente-x\nNa sexta (25/09): avancei Importar planilha.\n\n" +
		"estudos · loja-web\nNa sexta (25/09): concluí Corrigir frete.\n\n" +
		"trabalho-x · loja-web\nNa sexta (25/09): concluí Tela de cupons."
	if texto != esperado {
		t.Errorf("texto da daily geral:\n%s\n\nesperado:\n%s", texto, esperado)
	}
}

func TestSprintGeralPorProjeto(t *testing.T) {
	eventos, c := geral(t)
	r := Sprint(eventos, em("2026-09-21 00:00"), em("2026-09-27 00:00"), c)
	var secoes []string
	for _, s := range r.Secoes {
		secoes = append(secoes, s.Projeto)
	}
	if got := strings.Join(secoes, " | "); got != "estudos · cliente-x | estudos · loja-web | trabalho-x · loja-web" {
		t.Errorf("seções da sprint: %s", got)
	}
	if r.Secoes[2].ProjetoID != 3 || r.Secoes[2].Workspace != "trabalho-x" || len(r.Secoes[2].Concluidas) != 1 {
		t.Errorf("seção do outro loja-web: %+v", r.Secoes[2])
	}
	if !strings.Contains(r.Markdown, "## trabalho-x · loja-web\n") {
		t.Errorf("markdown sem o workspace na seção: %s", r.Markdown)
	}

	// Um workspace só no recorte: sem o prefixo.
	c.VariosWorkspaces = false
	if r := Sprint(eventos, em("2026-09-21 00:00"), em("2026-09-27 00:00"), c); r.Secoes[0].Projeto != "cliente-x" || r.Secoes[0].Workspace != "" {
		t.Errorf("seção sem prefixo: %+v", r.Secoes[0])
	}
	// A linha do tempo também diz o workspace na etiqueta.
	c.VariosWorkspaces = true
	dias := Montar(eventos, c)
	if dias[0].Itens[0].Projeto != "estudos · cliente-x" {
		t.Errorf("etiqueta na linha do tempo: %+v", dias[0].Itens[0])
	}
}
