package linha

import (
	"encoding/json"
	"flag"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

var atualizar = flag.Bool("atualizar", false, "regrava os arquivos de referência em testdata/")

// Fuso fixo: os testes não dependem de onde rodam.
var fuso = time.FixedZone("BRT", -3*3600)

// historico fabrica eventos em ordem, com momentos no fuso local de teste.
type historico struct {
	t       *testing.T
	eventos []dados.Evento
}

func (h *historico) add(momento, tipo string, e dados.Escopo, conteudo map[string]any) {
	h.t.Helper()
	quando, err := time.ParseInLocation("2006-01-02 15:04", momento, fuso)
	if err != nil {
		h.t.Fatal(err)
	}
	bruto, _ := json.Marshal(conteudo)
	h.eventos = append(h.eventos, dados.Evento{ID: int64(len(h.eventos) + 1), Momento: quando.UTC().Format(time.RFC3339Nano), Tipo: tipo, Dados: bruto, Escopo: e})
}

func tarefa(id int64, titulo, coluna string) map[string]any {
	return map[string]any{"id": id, "titulo": titulo, "coluna": coluna, "projeto_id": 1}
}

func em(dia string) time.Time {
	t, _ := time.ParseInLocation("2006-01-02 15:04", dia, fuso)
	return t
}

// Uma semana de trabalho em loja-web: segunda é 28/09/2026, sexta foi 25/09.
func semana(t *testing.T) []dados.Evento {
	h := &historico{t: t}
	p := dados.Escopo{Perfil: 1, Projeto: 1}
	t1 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1}
	t2 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 2}
	h.add("2026-09-21 09:00", "projeto.criado", p, map[string]any{"id": 1, "nome": "loja-web"})
	h.add("2026-09-21 09:05", "tarefa.criada", t1, map[string]any{"id": 1, "titulo": "Nova tela de pedidos", "projeto_id": 1, "projeto_nome": "loja-web"})
	h.add("2026-09-21 09:06", "tarefa.criada", t2, map[string]any{"id": 2, "titulo": "Corrigir desconto", "projeto_id": 1, "projeto_nome": "loja-web"})
	// Sexta: o agente trabalha na tela de pedidos, que vai para Revisão em
	// três movimentos seguidos; o desconto é concluído.
	a1 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 7}
	h.add("2026-09-25 10:00", "agente.iniciou", a1, map[string]any{"ferramenta": "claude", "papel": "dev", "titulo": "Nova tela de pedidos"})
	h.add("2026-09-25 10:00", "tarefa.atualizada", t1, map[string]any{"tarefa": tarefa(1, "Nova tela de pedidos", "trabalhando"), "coluna_antes": "backlog", "origem": "automatico"})
	h.add("2026-09-25 11:17", "agente.terminou", a1, map[string]any{"ferramenta": "claude", "papel": "dev", "titulo": "Nova tela de pedidos", "motivo": "terminou", "trabalhando_s": 3900, "aguardando_s": 720})
	h.add("2026-09-25 11:20", "tarefa.atualizada", t1, map[string]any{"tarefa": tarefa(1, "Nova tela de pedidos", "aguardando"), "coluna_antes": "trabalhando", "origem": "voce"})
	h.add("2026-09-25 11:22", "tarefa.atualizada", t1, map[string]any{"tarefa": tarefa(1, "Nova tela de pedidos", "trabalhando"), "coluna_antes": "aguardando", "origem": "voce"})
	h.add("2026-09-25 11:25", "tarefa.atualizada", t1, map[string]any{"tarefa": tarefa(1, "Nova tela de pedidos", "revisao"), "coluna_antes": "trabalhando", "origem": "voce"})
	h.add("2026-09-25 15:00", "tarefa.atualizada", t2, map[string]any{"tarefa": tarefa(2, "Corrigir desconto", "concluido"), "coluna_antes": "backlog", "origem": "voce"})
	a2 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 2, Agente: 8}
	h.add("2026-09-25 16:00", "agente.terminou", a2, map[string]any{"ferramenta": "codex", "papel": "testador", "titulo": "Corrigir desconto", "motivo": "erro", "codigo": 1, "trabalhando_s": 300})
	h.add("2026-09-25 16:05", "anexo.adicionado", t2, map[string]any{"anexo": 3, "origem": "captura", "ferramenta": "codex", "papel": "testador", "titulo": "Corrigir desconto"})
	// Segunda de manhã, um agente começou e ainda roda.
	a3 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 9}
	h.add("2026-09-28 08:30", "agente.iniciou", a3, map[string]any{"ferramenta": "claude", "papel": "revisor", "titulo": "Nova tela de pedidos"})
	return h.eventos
}

func contexto() Contexto {
	return Contexto{
		Agora: em("2026-09-28 09:00"), Fuso: fuso,
		Projetos: map[int64]string{1: "loja-web"},
		Tarefas: map[int64]TarefaAtual{
			1: {ID: 1, Titulo: "Nova tela de pedidos", Coluna: "aguardando", ProjetoID: 1},
			2: {ID: 2, Titulo: "Corrigir desconto", Coluna: "concluido", ProjetoID: 1},
		},
		Ativos:        map[int64]Ativo{9: {Estado: "trabalhando", Tarefa: 1}},
		NomesNoEscopo: []string{"loja-web"},
		MostrarTempo:  true,
	}
}

func TestLinhaDoTempo(t *testing.T) {
	dias := Montar(semana(t), contexto())
	if len(dias) != 3 || dias[0].Titulo != "Hoje · segunda, 28 de setembro" || dias[1].Titulo != "sexta, 25 de setembro" {
		t.Fatalf("dias: %+v", dias)
	}
	hoje := dias[0].Itens
	if len(hoje) != 1 || hoje[0].Tipo != TipoSessaoAberta || hoje[0].Texto != "Claude Code (revisor) trabalhando agora em “Nova tela de pedidos” desde 08:30." {
		t.Errorf("sessão aberta: %+v", hoje)
	}
	sexta := dias[1]
	textos := []string{}
	for _, item := range sexta.Itens {
		textos = append(textos, item.Hora+" "+item.Texto)
	}
	esperado := []string{
		"16:05 Captura do terminal de Codex (testador) em “Corrigir desconto”.",
		"16:00 Codex (testador) parou com erro (código 1) em “Corrigir desconto”.",
		"15:00 Concluiu “Corrigir desconto”.",
		// Três movimentos em menos de 10 minutos: uma linha, com a coluna final.
		"11:25 “Nova tela de pedidos” foi para Revisão.",
		"11:17 Claude Code (dev) trabalhou 1h05 em “Nova tela de pedidos” e esperou você 12 min.",
	}
	if strings.Join(textos, "\n") != strings.Join(esperado, "\n") {
		t.Errorf("sexta:\n%s\n\nesperado:\n%s", strings.Join(textos, "\n"), strings.Join(esperado, "\n"))
	}
	if sexta.Resumo != "1 concluída · agentes 1h10" {
		t.Errorf("resumo da sexta: %q", sexta.Resumo)
	}
	if sexta.Itens[0].Anexos[0] != 3 || sexta.Itens[0].AgenteID != 0 {
		t.Errorf("captura: %+v", sexta.Itens[0])
	}
}

func TestDailyNaSegundaMostraASexta(t *testing.T) {
	d := Daily(semana(t), contexto())
	esperado := "Na sexta (25/09): concluí Corrigir desconto; avancei Nova tela de pedidos; tive erro em Corrigir desconto (agentes trabalharam 1h10).\n" +
		"Hoje: Nova tela de pedidos está esperando minha resposta."
	if d.Texto != esperado {
		t.Errorf("texto:\n%s\n\nesperado:\n%s", d.Texto, esperado)
	}
	if d.Periodo != "Desde sexta (25/09) · loja-web" || d.Vazio {
		t.Errorf("período: %q", d.Periodo)
	}
	if d.Ontem == nil || len(d.Ontem.Blocos) != 3 || d.Ontem.Blocos[0].Titulo != "Concluídas · 1" {
		t.Errorf("blocos: %+v", d.Ontem)
	}
}

func TestDailyVazia(t *testing.T) {
	c := contexto()
	c.Tarefas = nil
	d := Daily(nil, c)
	if !d.Vazio || d.Texto != "Sem atividade nos últimos 7 dias." {
		t.Errorf("daily vazia: %+v", d)
	}
}

func TestDailyComVariosProjetosCitaOProjeto(t *testing.T) {
	c := contexto()
	c.VariosProjetos = true
	d := Daily(semana(t), c)
	// Os blocos citam o projeto; o texto sai separado por projeto.
	if d.Ontem == nil || d.Ontem.Blocos[0].Itens[0].Texto != "Corrigir desconto (loja-web)" {
		t.Errorf("sem o projeto nos blocos: %+v", d.Ontem)
	}
	if !strings.HasPrefix(d.Texto, "loja-web\nNa sexta (25/09): concluí Corrigir desconto;") {
		t.Errorf("texto sem o projeto em cima: %s", d.Texto)
	}
}

func TestSprint(t *testing.T) {
	r := Sprint(semana(t), em("2026-09-21 00:00"), em("2026-09-27 00:00"), contexto())
	if r.Vazio || len(r.Secoes) != 1 || len(r.Capturas) != 1 {
		t.Fatalf("sprint: %+v", r)
	}
	s := r.Secoes[0]
	if len(s.Concluidas) != 1 || s.Concluidas[0].Texto != "Corrigir desconto (25/09)" || len(s.Andamento) != 1 || len(s.Criadas) != 2 {
		t.Errorf("seção: %+v", s)
	}
	if s.Andamento[0].Texto != "Nova tela de pedidos (Revisão)" {
		t.Errorf("em andamento no fim do período: %+v", s.Andamento)
	}
	// Uma mudança automática também conta para onde a tarefa estava.
	h := &historico{t: t, eventos: semana(t)}
	h.add("2026-09-26 10:00", "tarefa.atualizada", dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1}, map[string]any{"tarefa": tarefa(1, "Nova tela de pedidos", "aguardando"), "coluna_antes": "revisao", "origem": "automatico"})
	if r := Sprint(h.eventos, em("2026-09-21 00:00"), em("2026-09-27 00:00"), contexto()); r.Secoes[0].Andamento[0].Texto != "Nova tela de pedidos (Aguardando você)" {
		t.Errorf("mudança automática ignorada: %+v", r.Secoes[0].Andamento)
	}
	referencia := "testdata/sprint.md"
	if *atualizar {
		os.WriteFile(referencia, []byte(r.Markdown), 0o644)
	}
	esperado, err := os.ReadFile(referencia)
	if err != nil {
		t.Fatal(err)
	}
	if r.Markdown != string(esperado) {
		t.Errorf("markdown mudou (rode com -atualizar se foi de propósito):\n%s", r.Markdown)
	}
	vazia := Sprint(semana(t), em("2026-08-01 00:00"), em("2026-08-14 00:00"), contexto())
	if !vazia.Vazio || vazia.Texto != "Nenhuma atividade de 01/08 a 14/08." {
		t.Errorf("sprint vazia: %+v", vazia.Texto)
	}
}

func TestEventosAntigosSemTitulo(t *testing.T) {
	h := &historico{t: t}
	// Como a entrega B gravava: "tarefa" é só o número.
	h.add("2026-09-28 08:00", "tarefa.atualizada", dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 5}, map[string]any{"tarefa": 5, "mudanca": map[string]any{"coluna": "concluido"}})
	h.add("2026-09-28 08:30", "tarefa.removida", dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 6}, map[string]any{"tarefa": 6})
	c := contexto()
	c.Tarefas[5] = TarefaAtual{ID: 5, Titulo: "Antiga", Coluna: "concluido", ProjetoID: 1}
	dias := Montar(h.eventos, c)
	if dias[0].Itens[0].Texto != "Removeu a tarefa “tarefa #6”." || !dias[0].Itens[0].Removida {
		t.Errorf("remoção antiga: %+v", dias[0].Itens[0])
	}
	if dias[0].Itens[1].Texto != "Concluiu “Antiga”." {
		t.Errorf("conclusão antiga: %+v", dias[0].Itens[1])
	}
}

func TestDuracao(t *testing.T) {
	for s, esperado := range map[int64]string{30: "menos de 1 min", 720: "12 min", 3900: "1h05", 7800: "2h10"} {
		if Duracao(s) != esperado {
			t.Errorf("%d s: %q", s, Duracao(s))
		}
	}
}

// Um projeto com atividade só hoje: a daily não pode dizer "sem atividade",
// e os blocos vão como lista mesmo vazios.
func TestDailyComAtividadeSoDeHoje(t *testing.T) {
	h := &historico{t: t}
	t3 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 3}
	h.add("2026-09-28 08:10", "tarefa.criada", t3, map[string]any{"id": 3, "titulo": "Relatório lento", "projeto_id": 1, "projeto_nome": "loja-web"})
	a := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 3, Agente: 4}
	h.add("2026-09-28 08:20", "agente.terminou", a, map[string]any{"ferramenta": "codex", "papel": "testador", "motivo": "erro", "codigo": 3})
	c := contexto()
	c.Tarefas = map[int64]TarefaAtual{3: {ID: 3, Titulo: "Relatório lento", Coluna: "revisao", ProjetoID: 1}}
	c.Ativos = nil
	d := Daily(h.eventos, c)
	esperado := "Hoje: criei Relatório lento; tive erro em Relatório lento."
	if d.Vazio || d.Texto != esperado {
		t.Errorf("texto: %q, esperado %q", d.Texto, esperado)
	}
	if len(d.Hoje.Blocos) != 2 || d.Hoje.Blocos[0].Titulo != "Criadas hoje · 1" {
		t.Errorf("blocos: %+v", d.Hoje.Blocos)
	}

	vazia := Daily(nil, Contexto{Agora: c.Agora, Fuso: fuso})
	bruto, _ := json.Marshal(vazia)
	if !strings.Contains(string(bruto), `"blocos":[]`) {
		t.Errorf("blocos precisam ir como lista: %s", bruto)
	}
}

// Terminais abertos e parados não são trabalho: nem "em andamento" na daily,
// nem "trabalhando agora" na linha do tempo.
func TestTerminalParadoNaoEhTrabalho(t *testing.T) {
	h := &historico{t: t}
	a := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 5, Agente: 11}
	h.add("2026-09-28 08:40", "agente.iniciou", a, map[string]any{"ferramenta": "shell", "papel": "dev", "titulo": "Ajustar build"})
	c := contexto()
	c.Tarefas = map[int64]TarefaAtual{5: {ID: 5, Titulo: "Ajustar build", Coluna: "trabalhando", ProjetoID: 1}}
	c.Ativos = map[int64]Ativo{11: {Estado: "ocioso", Desde: em("2026-09-28 08:41"), Tarefa: 5}}
	if d := Daily(h.eventos, c); !d.Vazio {
		t.Errorf("terminal parado virou trabalho na daily: %q", d.Texto)
	}
	dias := Montar(h.eventos, c)
	item := dias[0].Itens[0]
	if item.Tipo != TipoSessaoParada || item.Texto != "Terminal (dev) parado em “Ajustar build” desde 08:41." {
		t.Errorf("sessão parada: %+v", item)
	}
}

func TestPedidosENavegadorNaLinha(t *testing.T) {
	h := &historico{t: t}
	t1 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1}
	a1 := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 7}
	h.add("2026-10-02 09:05", "tarefa.criada", t1, map[string]any{"id": 1, "titulo": "Nova tela de pedidos", "projeto_id": 1, "projeto_nome": "loja-web"})
	h.add("2026-10-02 21:40", "pedido.criado", a1, map[string]any{"pedido": 3, "tamanho": 23})
	h.add("2026-10-02 21:41", "pedido.entregue", a1, map[string]any{"pedido": 3, "tamanho": 23})
	h.add("2026-10-02 21:42", "navegador.aberto", a1, map[string]any{"descricao": "localhost:5173/pedidos"})
	h.add("2026-10-02 21:43", "nota.atualizada", a1, map[string]any{"tipo": "daily", "periodo": "2026-10-02", "tamanho": 40, "agente": 7, "modo": "complementar"})
	h.add("2026-10-02 21:44", "pedido.respondido", a1, map[string]any{"pedido": 3, "tamanho": 23})
	h.add("2026-10-02 21:49", "navegador.recusado", a1, map[string]any{})
	h.add("2026-10-02 21:50", "navegador.fechado", t1, map[string]any{})
	h.add("2026-10-02 22:00", "pedido.falhou", a1, map[string]any{"pedido": 4, "tamanho": 5, "motivo": "o agente foi parado pela Colmeia"})
	c := Contexto{Agora: em("2026-10-02 23:00"), Fuso: fuso, Projetos: map[int64]string{1: "loja-web"}, Tarefas: map[int64]TarefaAtual{1: {ID: 1, Titulo: "Nova tela de pedidos", Coluna: "trabalhando", ProjetoID: 1}},
		Pedidos: map[int64]string{3: "Traga o total de testes", 4: "Outro\npedido"}}
	var textos []string
	for _, d := range Montar(h.eventos, c) {
		for _, i := range d.Itens {
			textos = append(textos, i.Tipo+": "+i.Texto)
		}
	}
	tudo := strings.Join(textos, "\n")
	for _, esperado := range []string{
		"pedido: Pediu ao agente: «Traga o total de testes» em “Nova tela de pedidos”.",
		"pedido_entregue: O agente de “Nova tela de pedidos” recebeu o pedido «Traga o total de testes».",
		"navegador: O agente abriu o navegador em localhost:5173/pedidos (“Nova tela de pedidos”).",
		"navegador: A captura do navegador de “Nova tela de pedidos” foi recusada: a página saiu da pasta da tarefa.",
		"nota_agente: O agente complementou a nota da daily de “Nova tela de pedidos”.",
		"pedido_respondido: O agente respondeu o pedido «Traga o total de testes» em “Nova tela de pedidos”.",
		"navegador_fechado: O navegador de “Nova tela de pedidos” fechou.",
		"pedido_falhou: O pedido «Outro pedido» em “Nova tela de pedidos” ficou sem resposta: o agente foi parado pela Colmeia.",
	} {
		if !strings.Contains(tudo, esperado) {
			t.Errorf("faltou %q em:\n%s", esperado, tudo)
		}
	}
}

func TestLousaDoAgenteNaLinhaDoTempo(t *testing.T) {
	h := &historico{t: t}
	a := dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1, Agente: 7}
	p := dados.Escopo{Perfil: 1}
	h.add("2026-09-25 10:00", "tarefa.criada", dados.Escopo{Perfil: 1, Projeto: 1, Tarefa: 1}, map[string]any{"id": 1, "titulo": "Nova tela de pedidos", "projeto_id": 1, "projeto_nome": "loja-web"})
	conteudo := map[string]any{"lousa": 3, "tarefa": 1, "titulo": "Nova tela de pedidos", "projeto_nome": "loja-web", "quantidade": 4,
		"tipos": map[string]int{"nota": 2, "ligacao": 2}, "ferramenta": "claude", "papel": "dev"}
	h.add("2026-09-25 10:10", "lousa.agente", a, conteudo)
	conteudo["quantidade"] = 2
	h.add("2026-09-25 10:12", "lousa.agente", a, conteudo)
	// Imagem posta numa lousa não aparece na linha do tempo.
	h.add("2026-09-25 10:15", "anexo.adicionado", p, map[string]any{"anexo": 9, "origem": "colagem", "na_lousa": true})
	c := Contexto{Agora: em("2026-09-25 18:00"), Fuso: fuso, Tarefas: map[int64]TarefaAtual{1: {ID: 1, Titulo: "Nova tela de pedidos", Coluna: "trabalhando", ProjetoID: 1}}}
	dias := Montar(h.eventos, c)
	var textos []string
	for _, item := range dias[0].Itens {
		textos = append(textos, item.Texto)
	}
	if len(textos) != 2 || textos[0] != "Claude Code (dev) acrescentou 6 itens à lousa de “Nova tela de pedidos”." {
		t.Errorf("itens: %q", textos)
	}
	if curto := dias[0].Itens[0].Curto; curto != "Claude Code (dev) acrescentou 6 itens à lousa." {
		t.Errorf("curto: %q", curto)
	}
	// Na apresentação, a lousa conta como trabalho na tarefa e o slide leva o resumo dela.
	deck := Apresentacao(h.eventos, em("2026-09-25 00:00"), em("2026-09-25 00:00"), c, "sprint")
	if len(deck.Slides) != 1 {
		t.Fatalf("slides: %+v", deck.Slides)
	}
	deck.CompletarLousas(map[int64]dados.ResumoLousa{1: {ID: 3, Elementos: 6}})
	if l := deck.Slides[0].Lousa; l == nil || l.ID != 3 || l.Elementos != 6 {
		t.Errorf("lousa no slide: %+v", l)
	}
	deck.Slides[0].Lousa = nil
	deck.CompletarLousas(map[int64]dados.ResumoLousa{1: {ID: 3, Elementos: 0}})
	if deck.Slides[0].Lousa != nil {
		t.Error("lousa vazia no slide")
	}
}

// As consultas do dia numa conexão viram um item com a hora da última, e a
// ordem do dia segue essa hora.
func TestConsultasJuntasNaHoraDaUltima(t *testing.T) {
	h := &historico{t: t}
	p := dados.Escopo{Perfil: 1}
	consulta := map[string]any{"conexao_id": 1, "conexao": "loja-web-dev", "tipo": "postgres", "verbo": "SELECT", "linhas": 3, "origem": "voce"}
	h.add("2026-09-25 14:05", "banco.consulta", p, consulta)
	h.add("2026-09-25 14:15", "projeto.criado", p, map[string]any{"id": 1, "nome": "loja-web"})
	h.add("2026-09-25 14:23", "banco.consulta", p, consulta)
	dias := Montar(h.eventos, Contexto{Agora: em("2026-09-25 18:00"), Fuso: fuso})
	itens := dias[0].Itens
	if len(itens) != 2 || itens[0].Hora != "14:23" || itens[0].Texto != "Consultou o banco loja-web-dev (2 vezes)." || itens[1].Hora != "14:15" {
		t.Fatalf("itens: %+v", itens)
	}
}
