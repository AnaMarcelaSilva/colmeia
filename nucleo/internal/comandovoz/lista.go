package comandovoz

import (
	"fmt"
	"sort"
	"strings"
)

// A lista numerada que a abelha fala quando é chamada: o que importa agora,
// numa ordem que não muda entre uma chamada e outra (primeiro o que espera
// você, depois o que está trabalhando, depois os erros; em cada grupo, a
// tarefa mais antiga antes). Os números valem dentro da página (1 a 5): a
// fala fica curta e a gramática dos comandos, pequena.

// PorPagina é quantos itens a abelha fala de uma vez.
const PorPagina = 5

// Grupo de um item, na ordem em que aparecem.
type Grupo int

const (
	GrupoEspera Grupo = iota
	GrupoTrabalhando
	GrupoErro
)

func (g Grupo) String() string {
	return [...]string{"espera", "trabalhando", "erro"}[g]
}

// ItemLista é uma tarefa da lista, com o agente que importa nela (o que
// espera você, o que trabalha ou o que parou com erro; 0 sem agente).
type ItemLista struct {
	TarefaID  int64
	ProjetoID int64
	AgenteID  int64
	Titulo    string
	Grupo     Grupo
	// Detalhe é o que vem depois do título: "o Claude Code espera você".
	Detalhe string
}

// Linha é o item como a abelha fala e o balão mostra, com o número da página.
func (i ItemLista) Linha(numero int) string {
	return fmt.Sprintf("%d. %s: %s.", numero, curto(i.Titulo), i.Detalhe)
}

// Os títulos longos ficam nas primeiras palavras (a fala e o balão são curtos).
const letrasDoTitulo = 40

func curto(titulo string) string {
	t := strings.Join(strings.Fields(titulo), " ")
	if len([]rune(t)) <= letrasDoTitulo {
		return t
	}
	var b strings.Builder
	for _, p := range strings.Fields(t) {
		if b.Len() > 0 && len([]rune(b.String()))+1+len([]rune(p)) > letrasDoTitulo {
			break
		}
		if b.Len() > 0 {
			b.WriteByte(' ')
		}
		b.WriteString(p)
	}
	if b.Len() == 0 {
		return string([]rune(t)[:letrasDoTitulo]) + "…"
	}
	return b.String() + "…"
}

// MontarLista junta, uma vez por tarefa e fora das concluídas, o que espera
// você (um agente aguardando ou a tarefa na coluna "aguardando"), o que tem
// agente trabalhando e o que tem agente parado com erro.
func MontarLista(q Quadro) []ItemLista {
	agentes := map[int64][]Agente{}
	for _, a := range q.Agentes {
		agentes[a.TarefaID] = append(agentes[a.TarefaID], a)
	}
	var itens []ItemLista
	for _, t := range q.Tarefas {
		if t.Coluna == "concluido" {
			continue
		}
		if item, ok := itemDaTarefa(t, agentes[t.ID]); ok {
			itens = append(itens, item)
		}
	}
	sort.SliceStable(itens, func(i, j int) bool {
		if itens[i].Grupo != itens[j].Grupo {
			return itens[i].Grupo < itens[j].Grupo
		}
		return itens[i].TarefaID < itens[j].TarefaID
	})
	return itens
}

// itemDaTarefa escolhe o grupo mais urgente da tarefa e o agente dele (o
// primeiro, pela ordem do quadro, quando há mais de um).
func itemDaTarefa(t Tarefa, agentes []Agente) (ItemLista, bool) {
	item := ItemLista{TarefaID: t.ID, ProjetoID: t.ProjetoID, Titulo: t.Titulo}
	var trabalhando, erro *Agente
	for i := range agentes {
		a := &agentes[i]
		switch {
		case a.Ativo && a.Estado == "aguardando":
			item.Grupo, item.AgenteID, item.Detalhe = GrupoEspera, a.ID, esperaDe(*a)
			return item, true
		case a.Ativo && trabalhando == nil:
			trabalhando = a
		case !a.Ativo && a.Erro && erro == nil:
			erro = a
		}
	}
	switch {
	case t.Coluna == "aguardando":
		item.Grupo, item.Detalhe = GrupoEspera, "espera você"
	case trabalhando != nil:
		item.Grupo, item.AgenteID, item.Detalhe = GrupoTrabalhando, trabalhando.ID, "trabalhando"
	case erro != nil:
		item.Grupo, item.AgenteID, item.Detalhe = GrupoErro, erro.ID, "o "+erro.Nome+" parou com erro"
	default:
		return item, false
	}
	return item, true
}

func esperaDe(a Agente) string {
	if a.Motivo == "" || strings.HasPrefix(a.Motivo, "esperando") {
		return "o " + a.Nome + " espera você"
	}
	return "o " + a.Nome + " " + a.Motivo
}

// Paginas diz quantas páginas a lista tem (ao menos uma, mesmo vazia).
func Paginas(itens []ItemLista) int {
	return max(1, (len(itens)+PorPagina-1)/PorPagina)
}

// Pagina devolve os itens da página (a partir de 0).
func Pagina(itens []ItemLista, pagina int) []ItemLista {
	inicio := pagina * PorPagina
	if pagina < 0 || inicio >= len(itens) {
		return nil
	}
	return itens[inicio:min(len(itens), inicio+PorPagina)]
}

// DicaDaLista é o que o balão ensina (a fala não repete, para ficar curta).
const DicaDaLista = "Diga, por exemplo: abrir a um, responder a dois, próxima ou fechar."

// FalaDaLista é o texto da página, como a abelha fala: os itens numerados
// e, havendo mais, como seguir.
func FalaDaLista(itens []ItemLista, pagina int) string {
	if len(itens) == 0 {
		return "Nada espera você e nenhum agente está trabalhando."
	}
	var partes []string
	if total := Paginas(itens); total > 1 {
		partes = append(partes, fmt.Sprintf("Página %d de %d.", pagina+1, total))
	}
	for n, item := range Pagina(itens, pagina) {
		partes = append(partes, item.Linha(n+1))
	}
	if pagina+1 < Paginas(itens) {
		partes = append(partes, "Diga próxima para ouvir mais.")
	}
	return strings.Join(partes, " ")
}

// VocabularioDaLista é o prompt do Whisper enquanto a lista está aberta: só
// as frases da gramática, para ele escrever "abrir a um" e não um nome
// parecido de tarefa ou projeto.
const VocabularioDaLista = "Abrir a um. Responder a dois. Concluir a três. Enviar para revisão a quatro. Parar a cinco. " +
	"Rodar a primeira. Abrir a segunda. Próxima. Anterior. Repetir. Fechar. Cancelar. Sim. Não."

// AcaoDaLista monta a resposta de um pedido com número ("concluir a dois")
// para o item da lista, com o quadro de agora. Concluir e parar perguntam
// antes, como os comandos por nome; abrir, responder e enviar para revisão
// acontecem na hora.
func AcaoDaLista(v Verbo, item ItemLista, q Quadro) Resposta {
	var t Tarefa
	achou := false
	for _, x := range q.Tarefas {
		if x.ID == item.TarefaID {
			t, achou = x, true
		}
	}
	if !achou {
		return Resposta{Texto: "Essa tarefa já não existe. Me chame de novo para ouvir a lista."}
	}
	switch v {
	case VAbrir:
		return abrir(t, q, true)
	case VResponder:
		return responderTarefa(t, item, q)
	case VConcluir:
		r := encerrar(t, q, true)
		r.Texto = strings.Replace(r.Texto, "Encerrar a tarefa", "Concluir a tarefa", 1)
		return r
	case VRevisao:
		if t.Coluna == "revisao" {
			return Resposta{Texto: fmt.Sprintf("A tarefa %s já está em Revisão.", t.Titulo)}
		}
		if t.Coluna == "concluido" {
			return Resposta{Texto: fmt.Sprintf("A tarefa %s já está em Concluído.", t.Titulo)}
		}
		return Resposta{Texto: fmt.Sprintf("Enviando a tarefa %s para Revisão.", t.Titulo),
			Acao: &Acao{Tipo: AcaoRevisao, TarefaID: t.ID, Feito: fmt.Sprintf("Pronto, a tarefa %s foi para Revisão.", t.Titulo)}}
	case VRodar, VParar:
		for _, p := range q.Projetos {
			if p.ID == t.ProjetoID {
				return doProjeto(v, p)
			}
		}
		return Resposta{Texto: "Não achei o projeto dessa tarefa."}
	}
	return Resposta{Texto: Ajuda}
}

// responderTarefa abre a tarefa com o ditado ligado na caixa do agente que
// espera você (ou do primeiro que está rodando).
func responderTarefa(t Tarefa, item ItemLista, q Quadro) Resposta {
	var agente *Agente
	for i := range q.Agentes {
		a := &q.Agentes[i]
		if a.TarefaID != t.ID || !a.Ativo {
			continue
		}
		if a.Estado == "aguardando" && (agente == nil || agente.Estado != "aguardando" || a.ID == item.AgenteID) {
			agente = a
		} else if agente == nil {
			agente = a
		}
	}
	if agente == nil {
		return Resposta{Texto: fmt.Sprintf("Nenhum agente está rodando na tarefa %s. Diga abrir para ver a tarefa.", t.Titulo), Ouvir: true}
	}
	feito := fmt.Sprintf("Abrindo a tarefa %s com o ditado ligado para o %s.", t.Titulo, agente.Nome)
	return Resposta{Texto: feito, Acao: &Acao{Tipo: AcaoResponder, TarefaID: t.ID, AgenteID: agente.ID, Feito: feito}}
}

// doProjeto roda ou para o projeto do item: com uma configuração só, ela;
// com várias, a abelha diz quais são para pedir pelo nome.
func doProjeto(v Verbo, p Projeto) Resposta {
	alvo := p.Execucoes
	if v == VParar {
		alvo = nil
		for _, e := range p.Execucoes {
			if e.Rodando {
				alvo = append(alvo, e)
			}
		}
	}
	switch len(alvo) {
	case 0:
		if v == VParar {
			return Resposta{Texto: fmt.Sprintf("Nada do projeto %s está rodando.", p.Nome)}
		}
		return Resposta{Texto: fmt.Sprintf("O projeto %s ainda não tem o que rodar. Crie uma configuração no Play do projeto.", p.Nome)}
	case 1:
		q := Quadro{Projetos: []Projeto{p}}
		if v == VParar {
			return parar(alvo[0].Nome+" "+p.Nome, q)
		}
		return rodar(alvo[0].Nome+" "+p.Nome, q)
	}
	var ns []string
	for _, e := range alvo {
		ns = append(ns, e.Nome)
	}
	verbo := "rodar"
	if v == VParar {
		verbo = "parar"
	}
	return Resposta{
		Texto: fmt.Sprintf("O projeto %s tem %s configurações: %s. Diga qual, por exemplo: %s %s %s.", p.Nome, numero(len(ns), true), lista(ns), verbo, ns[0], p.Nome),
		Ouvir: true,
	}
}
