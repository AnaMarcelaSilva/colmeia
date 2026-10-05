package linha

import (
	"fmt"
	"slices"
	"sort"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
)

// Itens por bloco do resumo; o resto vira "e mais N".
const maxPorBloco = 5

// "Ontem" da daily é o último dia com atividade, olhando até esse tanto para trás.
const JanelaDaily = 7

// Maior período de uma sprint, em dias.
const MaxDiasSprint = 92

// ItemResumo é uma linha clicável de um bloco, para conferir antes de falar.
type ItemResumo struct {
	Texto    string `json:"texto"`
	Tipo     string `json:"tipo"`
	TarefaID int64  `json:"tarefa_id,omitempty"`
	AgenteID int64  `json:"agente_id,omitempty"`
	Removida bool   `json:"removida,omitempty"`
}

type Bloco struct {
	Titulo string       `json:"titulo"`
	Itens  []ItemResumo `json:"itens"`
	Mais   int          `json:"mais"`
}

type ParteDaily struct {
	Dia    string  `json:"dia,omitempty"`
	Titulo string  `json:"titulo"`
	Blocos []Bloco `json:"blocos"`
}

type ResumoDaily struct {
	Periodo string      `json:"periodo"`
	Ontem   *ParteDaily `json:"ontem,omitempty"`
	Hoje    ParteDaily  `json:"hoje"`
	Texto   string      `json:"texto"`
	Vazio   bool        `json:"vazio"`
	// TempoAgentes: o texto pode ter o tempo dos agentes (a opção do perfil).
	TempoAgentes bool `json:"tempo_agentes"`
}

// lista junta "a, b e c", com "e mais N" depois de maxPorBloco.
func lista(nomes []string) string {
	if len(nomes) > maxPorBloco {
		return strings.Join(nomes[:maxPorBloco], ", ") + fmt.Sprintf(" e mais %d", len(nomes)-maxPorBloco)
	}
	if len(nomes) <= 1 {
		return strings.Join(nomes, "")
	}
	return strings.Join(nomes[:len(nomes)-1], ", ") + " e " + nomes[len(nomes)-1]
}

// nomeNoTexto: o título, com o projeto quando há vários no escopo (e o
// texto não está separado por projeto).
func (m *montador) nomeNoTexto(tarefa, projeto int64, comProjeto bool) string {
	nome := m.titulo(tarefa)
	if p := m.nomeProjeto(projeto); comProjeto && p != "" {
		nome += " (" + p + ")"
	}
	return nome
}

// juntador guarda tarefas distintas na ordem em que aparecem.
type juntador struct {
	vistos map[int64]bool
	itens  []ItemResumo
	nomes  []string
}

func (j *juntador) por(id int64, item ItemResumo, nome string) {
	if j.vistos == nil {
		j.vistos = map[int64]bool{}
	}
	if id != 0 && j.vistos[id] {
		return
	}
	j.vistos[id] = true
	j.itens = append(j.itens, item)
	j.nomes = append(j.nomes, nome)
}

func (j juntador) bloco(titulo string) Bloco {
	b := Bloco{Titulo: fmt.Sprintf("%s · %d", titulo, len(j.itens)), Itens: j.itens}
	if len(b.Itens) > maxPorBloco {
		b.Itens, b.Mais = b.Itens[:maxPorBloco], len(j.itens)-maxPorBloco
	}
	return b
}

func resumoDe(item Item, texto string) ItemResumo {
	return ItemResumo{Texto: texto, Tipo: item.Tipo, TarefaID: item.TarefaID, AgenteID: item.AgenteID, Removida: item.Removida}
}

// Daily monta o "o que fiz ontem, o que faço hoje" a partir dos eventos dos
// últimos dias (JanelaDaily + hoje) e das tarefas abertas agora.
//
// No recorte com mais de um projeto (um workspace, o perfil inteiro), os
// blocos citam o projeto junto da tarefa e o texto sai separado por
// projeto, na ordem da barra lateral, para falar um de cada vez.
func Daily(eventos []dados.Evento, c Contexto) ResumoDaily {
	m := novoMontador(c)
	itens := m.montar(eventos)
	fuso := m.c.Fuso
	hoje := diaDe(c.Agora, fuso)
	limite := diaDe(c.Agora.In(fuso).AddDate(0, 0, -JanelaDaily), fuso)

	// O último dia antes de hoje com atividade de verdade (não só uma captura ou um projeto novo).
	ultimo := ""
	for _, item := range itens {
		dia := diaDe(item.quando, fuso)
		if dia < hoje && dia >= limite && item.Tipo != TipoProjeto && !deBanco(item.Tipo) && dia > ultimo {
			ultimo = dia
		}
	}
	todos := func(int64) bool { return true }
	r := m.daily(itens, ultimo, todos, c.VariosProjetos)
	r.TempoAgentes = c.MostrarTempo
	if !c.VariosProjetos || r.Vazio {
		return r
	}
	// O texto por projeto: os projetos com algo no texto, na ordem da lateral.
	vistos := map[int64]bool{}
	var projetos []int64
	marcar := func(id int64) {
		if !vistos[id] {
			vistos[id] = true
			projetos = append(projetos, id)
		}
	}
	for _, item := range itens {
		if dia := diaDe(item.quando, fuso); dia == hoje || dia == ultimo {
			marcar(item.ProjetoID)
		}
	}
	for _, t := range c.Tarefas {
		marcar(t.ProjetoID)
	}
	sort.Slice(projetos, func(i, j int) bool {
		a, b := projetos[i], projetos[j]
		return c.antesNaOrdem(a, b, m.nomeProjeto(a), m.nomeProjeto(b))
	})
	var partes []string
	for _, id := range projetos {
		so := m.daily(itens, ultimo, func(p int64) bool { return p == id }, false)
		if so.Vazio {
			continue
		}
		partes = append(partes, c.rotuloSecao(id, m.nomeProjeto(id))+"\n"+so.Texto)
	}
	if len(partes) > 0 {
		r.Texto = strings.Join(partes, "\n\n")
	}
	return r
}

// daily monta os blocos e o texto da daily com os itens dos projetos que
// `incluir` aceita; `ultimo` é o dia anterior com atividade ("" se não há).
func (m *montador) daily(itens []Item, ultimo string, incluir func(projeto int64) bool, comProjeto bool) ResumoDaily {
	c := m.c
	fuso := m.c.Fuso
	hoje := diaDe(c.Agora, fuso)
	ontemDeFato := diaDe(c.Agora.In(fuso).AddDate(0, 0, -1), fuso)

	// Blocos sempre como lista (vazia, nunca null): a tela espera uma lista.
	r := ResumoDaily{Hoje: ParteDaily{Titulo: "Hoje", Blocos: []Bloco{}}}
	var partesTexto []string

	if ultimo != "" {
		var concluidas, avancaram, erros juntador
		var segundos int64
		for _, item := range itens {
			if diaDe(item.quando, fuso) != ultimo || !incluir(item.ProjetoID) {
				continue
			}
			nome := m.nomeNoTexto(item.TarefaID, item.ProjetoID, comProjeto)
			switch item.Tipo {
			case TipoConcluiu:
				concluidas.por(item.TarefaID, resumoDe(item, nome), nome)
			case TipoMoveu, TipoSessao, TipoInterrompido:
				avancaram.por(item.TarefaID, resumoDe(item, nome), nome)
			case TipoErro:
				erros.por(item.TarefaID, resumoDe(item, item.Texto), nome)
			}
			segundos += item.trabalhando
		}
		// Quem concluiu não "avançou": já está em Concluídas.
		var soAvancaram juntador
		for i, it := range avancaram.itens {
			if !concluidas.vistos[it.TarefaID] {
				soAvancaram.por(it.TarefaID, it, avancaram.nomes[i])
			}
		}
		parte := &ParteDaily{Dia: ultimo, Titulo: rotuloDoDia(ultimo, ontemDeFato, fuso, true), Blocos: []Bloco{}}
		var frases []string
		if len(concluidas.itens) > 0 {
			parte.Blocos = append(parte.Blocos, concluidas.bloco("Concluídas"))
			frases = append(frases, "concluí "+lista(concluidas.nomes))
		}
		if len(soAvancaram.itens) > 0 {
			parte.Blocos = append(parte.Blocos, soAvancaram.bloco("Avançaram"))
			frases = append(frases, "avancei "+lista(soAvancaram.nomes))
		}
		if len(erros.itens) > 0 {
			parte.Blocos = append(parte.Blocos, erros.bloco("Com erro"))
			frases = append(frases, "tive erro em "+lista(erros.nomes))
		}
		if len(frases) > 0 {
			frase := strings.Join(frases, "; ")
			if segundos >= 60 {
				frase += " (agentes trabalharam " + Duracao(segundos) + ")"
			}
			partesTexto = append(partesTexto, parte.Titulo+": "+frase+".")
			r.Ontem = parte
		}
	}

	// Hoje: o que está aberto agora e tudo o que já aconteceu hoje.
	// Uma tarefa entra num bloco só, o primeiro que a pegar nesta ordem:
	// concluída, esperando você, em andamento, criada, avançou. Erros
	// aparecem sempre (como no dia anterior).
	var jaHoje, esperando, seguindo, criadas, avancaram, erros juntador
	usadas := map[int64]bool{}
	pegar := func(j *juntador, id int64, item ItemResumo, nome string) {
		if id != 0 && usadas[id] {
			return
		}
		usadas[id] = true
		j.por(id, item, nome)
	}
	for _, item := range itens {
		if diaDe(item.quando, fuso) == hoje && item.Tipo == TipoConcluiu && incluir(item.ProjetoID) {
			nome := m.nomeNoTexto(item.TarefaID, item.ProjetoID, comProjeto)
			pegar(&jaHoje, item.TarefaID, resumoDe(item, nome), nome)
		}
	}
	abertas := make([]TarefaAtual, 0, len(c.Tarefas))
	for _, t := range c.Tarefas {
		if incluir(t.ProjetoID) {
			abertas = append(abertas, t)
		}
	}
	sort.Slice(abertas, func(i, j int) bool { return abertas[i].ID < abertas[j].ID })
	for _, colunaDaVez := range []string{"aguardando", "trabalhando"} {
		for _, t := range abertas {
			if t.Coluna != colunaDaVez || (t.Coluna == "trabalhando" && soTerminaisParados(c.Ativos, t.ID)) {
				continue
			}
			nome := m.nomeNoTexto(t.ID, t.ProjetoID, comProjeto)
			item := ItemResumo{Texto: nome, TarefaID: t.ID, Tipo: t.Coluna}
			if t.Coluna == "aguardando" {
				pegar(&esperando, t.ID, item, nome)
			} else {
				pegar(&seguindo, t.ID, item, nome)
			}
		}
	}
	var segundosHoje int64
	for _, item := range itens {
		if diaDe(item.quando, fuso) != hoje || !incluir(item.ProjetoID) {
			continue
		}
		nome := m.nomeNoTexto(item.TarefaID, item.ProjetoID, comProjeto)
		switch item.Tipo {
		case TipoCriou:
			pegar(&criadas, item.TarefaID, resumoDe(item, nome), nome)
		case TipoErro:
			erros.por(item.TarefaID, resumoDe(item, item.Texto), nome)
		}
		segundosHoje += item.trabalhando
	}
	for _, item := range itens {
		if diaDe(item.quando, fuso) != hoje || !incluir(item.ProjetoID) {
			continue
		}
		switch item.Tipo {
		case TipoMoveu, TipoSessao, TipoInterrompido:
			nome := m.nomeNoTexto(item.TarefaID, item.ProjetoID, comProjeto)
			pegar(&avancaram, item.TarefaID, resumoDe(item, nome), nome)
		}
	}
	var frases []string
	if len(jaHoje.itens) > 0 {
		r.Hoje.Blocos = append(r.Hoje.Blocos, jaHoje.bloco("Concluídas hoje"))
		frases = append(frases, "já concluí "+lista(jaHoje.nomes))
	}
	if len(avancaram.itens) > 0 {
		r.Hoje.Blocos = append(r.Hoje.Blocos, avancaram.bloco("Avançaram hoje"))
		frases = append(frases, "avancei "+lista(avancaram.nomes))
	}
	if len(criadas.itens) > 0 {
		r.Hoje.Blocos = append(r.Hoje.Blocos, criadas.bloco("Criadas hoje"))
		frases = append(frases, "criei "+lista(criadas.nomes))
	}
	if len(erros.itens) > 0 {
		r.Hoje.Blocos = append(r.Hoje.Blocos, erros.bloco("Com erro hoje"))
		frases = append(frases, "tive erro em "+lista(erros.nomes))
	}
	if len(seguindo.itens) > 0 {
		r.Hoje.Blocos = append(r.Hoje.Blocos, seguindo.bloco("Em andamento"))
		frases = append(frases, "sigo em "+lista(seguindo.nomes))
	}
	if len(esperando.itens) > 0 {
		r.Hoje.Blocos = append(r.Hoje.Blocos, esperando.bloco("Esperando você"))
		verbo := " está esperando minha resposta"
		if len(esperando.nomes) > 1 {
			verbo = " estão esperando minha resposta"
		}
		frases = append(frases, lista(esperando.nomes)+verbo)
	}
	if len(frases) > 0 {
		frase := strings.Join(frases, "; ")
		if segundosHoje >= 60 {
			frase += " (agentes trabalharam " + Duracao(segundosHoje) + ")"
		}
		partesTexto = append(partesTexto, "Hoje: "+frase+".")
	}

	r.Texto = strings.Join(partesTexto, "\n")
	if r.Texto == "" {
		r.Vazio, r.Texto = true, fmt.Sprintf("Sem atividade nos últimos %d dias.", JanelaDaily)
	}
	desde := "Hoje"
	if r.Ontem != nil {
		desde = "Desde " + rotuloDoDia(r.Ontem.Dia, ontemDeFato, fuso, false)
	}
	r.Periodo = desde
	if e := c.escopo(); e != "" {
		r.Periodo += " · " + e
	}
	return r
}

// soTerminaisParados diz se a tarefa tem agentes rodando e todos estão
// parados (terminais comuns quietos): aberta assim, ela não está "em andamento".
func soTerminaisParados(ativos map[int64]Ativo, tarefa int64) bool {
	algum := false
	for _, a := range ativos {
		if a.Tarefa != tarefa {
			continue
		}
		if a.Estado != "ocioso" {
			return false
		}
		algum = true
	}
	return algum
}

// rotuloDoDia: "Ontem", ou "Na sexta (25/09)" quando o último dia não foi ontem.
func rotuloDoDia(dia, ontem string, fuso *time.Location, maiuscula bool) string {
	if dia == ontem {
		if maiuscula {
			return "Ontem"
		}
		return "ontem"
	}
	d, _ := time.ParseInLocation("2006-01-02", dia, fuso)
	texto := fmt.Sprintf("%s (%s)", diasSemana[d.Weekday()], d.Format("02/01"))
	if maiuscula {
		artigo := "Na "
		if d.Weekday() == time.Saturday || d.Weekday() == time.Sunday {
			artigo = "No "
		}
		return artigo + texto
	}
	return texto
}

// Sprint

type SecaoSprint struct {
	// Projeto é o nome da seção ("estudos · loja-web" com mais de um
	// workspace no recorte); ProjetoID e Workspace dizem qual é.
	Projeto    string       `json:"projeto"`
	ProjetoID  int64        `json:"projeto_id,omitempty"`
	Workspace  string       `json:"workspace,omitempty"`
	Concluidas []ItemResumo `json:"concluidas"`
	Andamento  []ItemResumo `json:"andamento"`
	Criadas    []ItemResumo `json:"criadas"`
	Removidas  []ItemResumo `json:"removidas"`
	Erros      []ItemResumo `json:"erros"`
	TempoS     int64        `json:"tempo_s,omitempty"`
}

type TempoFerramenta struct {
	Nome    string `json:"nome"`
	Segundo int64  `json:"segundos"`
}

type Captura struct {
	Anexo    int64  `json:"anexo"`
	TarefaID int64  `json:"tarefa_id,omitempty"`
	Texto    string `json:"texto"`
	Dia      string `json:"dia"`
}

type ResumoSprint struct {
	De            string            `json:"de"`
	Ate           string            `json:"ate"`
	Periodo       string            `json:"periodo"`
	Secoes        []SecaoSprint     `json:"secoes"`
	TempoTotalS   int64             `json:"tempo_total_s,omitempty"`
	PorFerramenta []TempoFerramenta `json:"por_ferramenta,omitempty"`
	Capturas      []Captura         `json:"capturas"`
	Texto         string            `json:"texto"`
	Markdown      string            `json:"markdown"`
	Vazio         bool              `json:"vazio"`
	TempoAgentes  bool              `json:"tempo_agentes"`
}

// Sprint resume o período [de, ate] (datas no fuso, inclusive). `eventos` é o
// histórico do escopo até o fim do período: os anteriores servem para saber
// em que coluna cada tarefa estava no fim.
func Sprint(eventos []dados.Evento, de, ate time.Time, c Contexto) ResumoSprint {
	m := novoMontador(c)
	itens := m.montar(eventos)
	fuso := m.c.Fuso
	inicio, fim := de.Format("2006-01-02"), ate.Format("2006-01-02")
	r := ResumoSprint{De: de.Format("02/01/2006"), Ate: ate.Format("02/01/2006"), Secoes: []SecaoSprint{}, Capturas: []Captura{}, TempoAgentes: c.MostrarTempo}
	r.Periodo = fmt.Sprintf("De %s a %s", de.Format("02/01"), ate.Format("02/01"))
	if e := c.escopo(); e != "" {
		r.Periodo += " · " + e
	}

	// Uma seção por projeto (pelo id: dois workspaces podem ter um loja-web cada).
	secoes := map[int64]*SecaoSprint{}
	secao := func(projeto int64, nome string) *SecaoSprint {
		if s, ok := secoes[projeto]; ok {
			return s
		}
		s := &SecaoSprint{Projeto: c.rotuloSecao(projeto, cmpOr(nome, m.nomeProjeto(projeto))), ProjetoID: projeto, Workspace: c.workspaceDaSecao(projeto)}
		secoes[projeto] = s
		return s
	}
	// Coluna de cada tarefa no fim do período, refeita pelo histórico
	// (inclusive pelas mudanças automáticas, que não viram item).
	coluna := map[int64]string{}
	projetoDa := map[int64]int64{}
	for _, mc := range m.colunas {
		if diaDe(mc.quando, fuso) > fim {
			break
		}
		coluna[mc.tarefa], projetoDa[mc.tarefa] = mc.coluna, mc.projeto
	}
	porFerramenta := map[string]int64{}
	concluidaEm := map[int64]bool{}
	for _, item := range itens {
		dia := diaDe(item.quando, fuso)
		if dia > fim {
			break
		}
		if dia < inicio {
			continue
		}
		s := secao(item.ProjetoID, item.Projeto)
		nome := m.titulo(item.TarefaID)
		switch item.Tipo {
		case TipoConcluiu:
			if !concluidaEm[item.TarefaID] {
				concluidaEm[item.TarefaID] = true
				s.Concluidas = append(s.Concluidas, resumoDe(item, fmt.Sprintf("%s (%s)", nome, item.quando.In(fuso).Format("02/01"))))
			}
		case TipoCriou:
			s.Criadas = append(s.Criadas, resumoDe(item, nome))
		case TipoRemoveu:
			s.Removidas = append(s.Removidas, resumoDe(item, nome))
		case TipoErro:
			s.Erros = append(s.Erros, resumoDe(item, strings.TrimSuffix(item.Texto, ".")+" ("+item.quando.In(fuso).Format("02/01")+")"))
		case TipoCaptura:
			for _, a := range item.Anexos {
				if slices.Contains(item.Videos, a) {
					continue // a sprint salva PNGs; o vídeo fica na apresentação
				}
				r.Capturas = append(r.Capturas, Captura{Anexo: a, TarefaID: item.TarefaID, Texto: strings.TrimSuffix(item.Texto, "."), Dia: item.quando.In(fuso).Format("02/01")})
			}
		}
		if item.trabalhando > 0 {
			s.TempoS += item.trabalhando
			r.TempoTotalS += item.trabalhando
			porFerramenta[ferramentas.Nome(item.ferramenta)] += item.trabalhando
		}
	}
	// Em andamento no fim do período: o que não terminou nem saiu.
	ids := make([]int64, 0, len(coluna))
	for id := range coluna {
		ids = append(ids, id)
	}
	sort.Slice(ids, func(i, j int) bool { return ids[i] < ids[j] })
	for _, id := range ids {
		col := coluna[id]
		if col != "trabalhando" && col != "aguardando" && col != "revisao" {
			continue
		}
		s := secao(projetoDa[id], "")
		s.Andamento = append(s.Andamento, ItemResumo{Texto: m.titulo(id) + " (" + nomesColuna[col] + ")", Tipo: col, TarefaID: id})
	}
	ordem := make([]*SecaoSprint, 0, len(secoes))
	for _, s := range secoes {
		ordem = append(ordem, s)
	}
	sort.Slice(ordem, func(i, j int) bool {
		return c.antesNaOrdem(ordem[i].ProjetoID, ordem[j].ProjetoID, ordem[i].Projeto, ordem[j].Projeto)
	})
	for _, s := range ordem {
		if len(s.Concluidas)+len(s.Andamento)+len(s.Criadas)+len(s.Removidas)+len(s.Erros) == 0 && s.TempoS == 0 {
			continue
		}
		r.Secoes = append(r.Secoes, *s)
	}
	for nome, segundos := range porFerramenta {
		r.PorFerramenta = append(r.PorFerramenta, TempoFerramenta{Nome: nome, Segundo: segundos})
	}
	sort.Slice(r.PorFerramenta, func(i, j int) bool {
		if r.PorFerramenta[i].Segundo != r.PorFerramenta[j].Segundo {
			return r.PorFerramenta[i].Segundo > r.PorFerramenta[j].Segundo
		}
		return r.PorFerramenta[i].Nome < r.PorFerramenta[j].Nome
	})
	r.Vazio = len(r.Secoes) == 0 && len(r.Capturas) == 0
	r.Texto, r.Markdown = textosSprint(r)
	return r
}

func textos(itens []ItemResumo) []string {
	nomes := make([]string, len(itens))
	for i, it := range itens {
		nomes[i] = it.Texto
	}
	return nomes
}

// textosSprint escreve o resumo em texto simples (para falar ou colar num
// chat) e em Markdown (para salvar com as capturas ao lado).
func textosSprint(r ResumoSprint) (string, string) {
	if r.Vazio {
		vazio := fmt.Sprintf("Nenhuma atividade de %s a %s.", r.De[:5], r.Ate[:5])
		return vazio, "# Sprint de " + r.De + " a " + r.Ate + "\n\n" + vazio + "\n"
	}
	var t, md strings.Builder
	fmt.Fprintf(&t, "Sprint de %s a %s\n", r.De[:5], r.Ate[:5])
	fmt.Fprintf(&md, "# Sprint de %s a %s\n", r.De, r.Ate)
	for _, s := range r.Secoes {
		fmt.Fprintf(&t, "\n%s\n", s.Projeto)
		fmt.Fprintf(&md, "\n## %s\n", s.Projeto)
		linha := func(rotulo string, itens []ItemResumo) {
			if len(itens) == 0 {
				return
			}
			fmt.Fprintf(&t, "• %s: %s\n", rotulo, strings.Join(textos(itens), ", "))
			fmt.Fprintf(&md, "\n**%s**\n\n", rotulo)
			for _, it := range itens {
				fmt.Fprintf(&md, "- %s\n", it.Texto)
			}
		}
		linha("Concluídas", s.Concluidas)
		linha("Em andamento", s.Andamento)
		if len(s.Criadas)+len(s.Removidas) > 0 {
			fmt.Fprintf(&t, "• Criadas: %d · Removidas: %d\n", len(s.Criadas), len(s.Removidas))
			fmt.Fprintf(&md, "\n**Criadas:** %d · **Removidas:** %d\n", len(s.Criadas), len(s.Removidas))
		}
		linha("Erros", s.Erros)
		if s.TempoS >= 60 {
			fmt.Fprintf(&t, "• Agentes: %s\n", Duracao(s.TempoS))
			fmt.Fprintf(&md, "\n**Tempo de agentes:** %s\n", Duracao(s.TempoS))
		}
	}
	if r.TempoTotalS >= 60 {
		partes := make([]string, 0, len(r.PorFerramenta))
		fmt.Fprintf(&md, "\n## Tempo de agentes\n\n")
		for _, f := range r.PorFerramenta {
			partes = append(partes, f.Nome+" "+Duracao(f.Segundo))
			fmt.Fprintf(&md, "- %s: %s\n", f.Nome, Duracao(f.Segundo))
		}
		fmt.Fprintf(&t, "\nAgentes no total: %s (%s)\n", Duracao(r.TempoTotalS), strings.Join(partes, ", "))
		fmt.Fprintf(&md, "- **Total:** %s\n", Duracao(r.TempoTotalS))
	}
	if len(r.Capturas) > 0 {
		fmt.Fprintf(&t, "\nCapturas: %d\n", len(r.Capturas))
		fmt.Fprintf(&md, "\n## Capturas\n\n")
		for _, c := range r.Capturas {
			fmt.Fprintf(&md, "![%s (%s)](capturas/%d.png)\n\n", c.Texto, c.Dia, c.Anexo)
		}
	}
	return strings.TrimRight(t.String(), "\n"), md.String()
}

// deBanco: suas consultas e alterações no banco não fazem de um dia um dia
// de trabalho nas tarefas (o pedido do agente, numa tarefa, faz).
func deBanco(tipo string) bool { return tipo == TipoBanco || tipo == TipoBancoAlteracao }
