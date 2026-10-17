package comandovoz

import (
	"cmp"
	"fmt"
	"strings"
)

// Execucao é uma configuração do Play de um projeto.
type Execucao struct {
	ID      int64
	Nome    string
	Rodando bool
}

type Projeto struct {
	ID        int64
	Nome      string
	Execucoes []Execucao
}

type Tarefa struct {
	ID        int64
	ProjetoID int64
	Titulo    string
	Coluna    string
}

// Agente com o que o núcleo sabe dele agora. Nome é o da ferramenta, como a
// tela mostra ("Claude Code").
type Agente struct {
	ID       int64
	TarefaID int64
	Nome     string
	Ativo    bool
	Estado   string
	Motivo   string
	// O último fim foi com erro (e ele não voltou a rodar).
	Erro bool
}

// Quadro é o retrato do perfil que as respostas usam.
type Quadro struct {
	Projetos []Projeto
	Tarefas  []Tarefa
	Agentes  []Agente
}

type TipoAcao string

const (
	AcaoRodar    TipoAcao = "rodar"
	AcaoParar    TipoAcao = "parar"
	AcaoEncerrar TipoAcao = "encerrar"
	AcaoAbrir    TipoAcao = "abrir"
)

// Acao é o que a API executa: na hora ou depois do "sim".
type Acao struct {
	Tipo       TipoAcao
	ExecucaoID int64
	TarefaID   int64
	// Feito é a frase para quando a ação deu certo.
	Feito string
}

// Resposta é a frase para o balão (e para a voz da abelha) e, se houver, a
// ação. Com Confirmar, a frase é a pergunta e a ação espera o "sim".
type Resposta struct {
	Texto     string
	Acao      *Acao
	Confirmar bool
}

// Ajuda é o que a abelha diz quando não entende.
const Ajuda = "Não entendi. Você pode pedir: rodar ou parar um projeto, abrir ou encerrar uma tarefa, como está uma tarefa e o que espera por você."

var nomesColuna = map[string]string{
	"backlog":     "no Backlog",
	"trabalhando": "com o agente trabalhando",
	"aguardando":  "aguardando você",
	"revisao":     "em Revisão",
	"concluido":   "em Concluído",
}

// Responder monta a resposta de um pedido com o quadro. "Sim" e "não"
// dependem da pergunta que ficou no ar: quem guarda isso é a API. Com o verbo
// corrigido, toda ação vira pergunta.
func Responder(p Pedido, q Quadro) Resposta {
	r := montar(p, q)
	if p.Duvida && r.Acao != nil && !r.Confirmar {
		r.Confirmar = true
		r.Texto = perguntaDe(r.Acao.Feito)
	}
	return r
}

// perguntaDe faz da frase de feito a pergunta: "Rodando o projeto x." vira
// "Rodar o projeto x?"; "Abrindo a tarefa y." vira "Abrir a tarefa y?".
func perguntaDe(feito string) string {
	f := strings.TrimSuffix(feito, ".")
	for de, para := range map[string]string{"Rodando ": "Rodar ", "Abrindo ": "Abrir "} {
		if strings.HasPrefix(f, de) {
			return para + strings.TrimPrefix(f, de) + "?"
		}
	}
	return f + "?"
}

func montar(p Pedido, q Quadro) Resposta {
	switch p.Intencao {
	case Rodar:
		return rodar(p.Alvo, q)
	case Parar:
		return parar(p.Alvo, q)
	case Encerrar:
		return comTarefa(p.Alvo, q, encerrar)
	case Abrir:
		return comTarefa(p.Alvo, q, abrir)
	case ComoEsta:
		return comTarefa(p.Alvo, q, comoEsta)
	case OQueEspera:
		return oQueEspera(q)
	case SoNome:
		return Resposta{Texto: "Estou ouvindo. Diga o nome e o pedido de uma vez, por exemplo: o que espera por mim?"}
	}
	return Resposta{Texto: Ajuda}
}

// comTarefa acha a tarefa pelo nome dito e segue; sem certeza, pergunta.
func comTarefa(dito string, q Quadro, seguir func(Tarefa, Quadro, bool) Resposta) Resposta {
	cands := make([]Candidato, 0, len(q.Tarefas))
	for _, t := range q.Tarefas {
		cands = append(cands, Candidato{ID: t.ID, Nome: t.Titulo})
	}
	b := Procurar(dito, cands)
	melhor, ok := b.Melhor()
	if !ok {
		return Resposta{Texto: fmt.Sprintf("Não achei uma tarefa com o nome %s.", dito)}
	}
	if e := b.Empate(); e != nil {
		return Resposta{Texto: fmt.Sprintf("Achei mais de uma tarefa parecida: %s. Diga o nome mais completo.", lista(nomes(e)))}
	}
	for _, t := range q.Tarefas {
		if t.ID == melhor.ID {
			return seguir(t, q, b.Certo())
		}
	}
	return Resposta{Texto: Ajuda}
}

func encerrar(t Tarefa, _ Quadro, _ bool) Resposta {
	if t.Coluna == "concluido" {
		return Resposta{Texto: fmt.Sprintf("A tarefa %s já está em Concluído.", t.Titulo)}
	}
	// Mexe no quadro: sempre pede o "sim".
	return Resposta{
		Texto:     fmt.Sprintf("Encerrar a tarefa %s? Ela vai para Concluído.", t.Titulo),
		Acao:      &Acao{Tipo: AcaoEncerrar, TarefaID: t.ID, Feito: fmt.Sprintf("Pronto, a tarefa %s foi para Concluído.", t.Titulo)},
		Confirmar: true,
	}
}

func abrir(t Tarefa, _ Quadro, certo bool) Resposta {
	a := &Acao{Tipo: AcaoAbrir, TarefaID: t.ID, Feito: fmt.Sprintf("Abrindo a tarefa %s.", t.Titulo)}
	if !certo {
		return Resposta{Texto: fmt.Sprintf("Abrir a tarefa %s?", t.Titulo), Acao: a, Confirmar: true}
	}
	return Resposta{Texto: a.Feito, Acao: a}
}

func comoEsta(t Tarefa, q Quadro, _ bool) Resposta {
	frase := fmt.Sprintf("A tarefa %s está %s.", t.Titulo, cmp.Or(nomesColuna[t.Coluna], "no quadro"))
	var esperam, trabalham, erros, parados []string
	for _, a := range q.Agentes {
		if a.TarefaID != t.ID {
			continue
		}
		switch {
		case a.Ativo && a.Estado == "aguardando":
			esperam = append(esperam, comMotivo(a))
		case a.Ativo:
			trabalham = append(trabalham, a.Nome)
		case a.Erro:
			erros = append(erros, a.Nome)
		default:
			parados = append(parados, a.Nome)
		}
	}
	partes := []string{frase}
	if len(esperam) > 0 {
		partes = append(partes, fmt.Sprintf("%s espera você: %s.", contar(len(esperam), "Um agente", "agentes"), lista(esperam)))
	}
	if len(trabalham) > 0 {
		partes = append(partes, fmt.Sprintf("%s: %s.", contar(len(trabalham), "Um agente trabalhando", "agentes trabalhando"), lista(trabalham)))
	}
	if len(erros) > 0 {
		partes = append(partes, fmt.Sprintf("%s com erro: %s.", contar(len(erros), "Um parou", "pararam"), lista(erros)))
	}
	if len(esperam)+len(trabalham)+len(erros) == 0 {
		if len(parados) > 0 {
			partes = append(partes, "Nenhum agente está rodando nela.")
		} else {
			partes = append(partes, "Ela ainda não tem agentes.")
		}
	}
	return Resposta{Texto: strings.Join(partes, " ")}
}

func comMotivo(a Agente) string {
	if a.Motivo == "" {
		return a.Nome
	}
	return a.Nome + ", que " + a.Motivo
}

// oQueEspera junta os agentes que esperam você e as tarefas em "Aguardando
// você", uma vez por tarefa.
func oQueEspera(q Quadro) Resposta {
	titulo := map[int64]string{}
	for _, t := range q.Tarefas {
		titulo[t.ID] = t.Titulo
	}
	var itens []string
	vista := map[int64]bool{}
	for _, a := range q.Agentes {
		if !a.Ativo || a.Estado != "aguardando" || vista[a.TarefaID] {
			continue
		}
		vista[a.TarefaID] = true
		item := titulo[a.TarefaID]
		if a.Motivo != "" {
			item += ", " + a.Motivo
		}
		itens = append(itens, item)
	}
	for _, t := range q.Tarefas {
		if t.Coluna == "aguardando" && !vista[t.ID] {
			vista[t.ID] = true
			itens = append(itens, t.Titulo)
		}
	}
	switch len(itens) {
	case 0:
		return Resposta{Texto: "Nada espera você agora."}
	case 1:
		return Resposta{Texto: fmt.Sprintf("Uma tarefa espera você: %s.", itens[0])}
	}
	return Resposta{Texto: fmt.Sprintf("%s esperam você: %s.", capitalizar(numero(len(itens), true)+" tarefas"), lista(itens))}
}

// execucoesCandidatas: cada configuração vale pelo nome dela, pelo do
// projeto junto e, quando é a única do projeto, só pelo nome do projeto.
func execucoesCandidatas(q Quadro) ([]Candidato, map[int64]Projeto) {
	var cands []Candidato
	dono := map[int64]Projeto{}
	for _, p := range q.Projetos {
		for _, e := range p.Execucoes {
			c := Candidato{ID: e.ID, Nome: e.Nome, Apelidos: []string{p.Nome + " " + e.Nome, e.Nome + " " + p.Nome}}
			if len(p.Execucoes) == 1 {
				c.Apelidos = append(c.Apelidos, p.Nome)
			}
			cands = append(cands, c)
			dono[e.ID] = p
		}
	}
	return cands, dono
}

func projetoCandidatos(q Quadro) []Candidato {
	var cands []Candidato
	for _, p := range q.Projetos {
		cands = append(cands, Candidato{ID: p.ID, Nome: p.Nome})
	}
	return cands
}

// qualExecucao acha a configuração dita. Sem uma clara, a resposta já diz
// o que falta (o projeto não tem nenhuma, tem várias ou não existe).
func qualExecucao(dito string, q Quadro) (Execucao, Projeto, bool, *Resposta) {
	cands, dono := execucoesCandidatas(q)
	bExec := Procurar(dito, cands)
	bProj := Procurar(dito, projetoCandidatos(q))
	mProj, temProj := bProj.Melhor()
	mExec, temExec := bExec.Melhor()
	// O nome é de um projeto (mais que de uma configuração): o projeto decide.
	if temProj && (!temExec || mProj.Nota > mExec.Nota) {
		for _, p := range q.Projetos {
			if p.ID != mProj.ID {
				continue
			}
			switch len(p.Execucoes) {
			case 0:
				return Execucao{}, p, false, &Resposta{Texto: fmt.Sprintf("O projeto %s ainda não tem o que rodar. Crie uma configuração no Play do projeto.", p.Nome)}
			case 1:
				return p.Execucoes[0], p, bProj.Certo(), nil
			}
			var ns []string
			for _, e := range p.Execucoes {
				ns = append(ns, e.Nome)
			}
			return Execucao{}, p, false, &Resposta{Texto: fmt.Sprintf("O projeto %s tem %s configurações: %s. Diga qual, por exemplo: %s.", p.Nome, numero(len(ns), false), lista(ns), ns[0])}
		}
	}
	if !temExec {
		return Execucao{}, Projeto{}, false, &Resposta{Texto: fmt.Sprintf("Não achei um projeto com o nome %s.", dito)}
	}
	if e := bExec.Empate(); e != nil {
		var ns []string
		for _, a := range e {
			ns = append(ns, a.Nome+" do projeto "+dono[a.ID].Nome)
		}
		return Execucao{}, Projeto{}, false, &Resposta{Texto: fmt.Sprintf("Achei mais de uma configuração parecida: %s. Diga o nome do projeto junto.", lista(ns))}
	}
	p := dono[mExec.ID]
	for _, e := range p.Execucoes {
		if e.ID == mExec.ID {
			return e, p, bExec.Certo(), nil
		}
	}
	return Execucao{}, Projeto{}, false, &Resposta{Texto: Ajuda}
}

func nomeDaExecucao(e Execucao, p Projeto) string {
	if len(p.Execucoes) == 1 || normalizar(e.Nome) == normalizar(p.Nome) {
		return "o projeto " + p.Nome
	}
	return e.Nome + ", do projeto " + p.Nome
}

func rodar(dito string, q Quadro) Resposta {
	e, p, certo, r := qualExecucao(dito, q)
	if r != nil {
		return *r
	}
	nome := nomeDaExecucao(e, p)
	a := &Acao{Tipo: AcaoRodar, ExecucaoID: e.ID, Feito: "Rodando " + nome + "."}
	if e.Rodando {
		// Rodar de novo para o que está rodando: pede o "sim".
		return Resposta{Texto: capitalizar(nome) + " já está rodando. Quer rodar de novo?", Acao: a, Confirmar: true}
	}
	if !certo {
		return Resposta{Texto: "Rodar " + nome + "?", Acao: a, Confirmar: true}
	}
	return Resposta{Texto: a.Feito, Acao: a}
}

func parar(dito string, q Quadro) Resposta {
	e, p, _, r := qualExecucao(dito, q)
	if r != nil {
		return *r
	}
	nome := nomeDaExecucao(e, p)
	if !e.Rodando {
		return Resposta{Texto: capitalizar(nome) + " não está rodando."}
	}
	// Parar derruba o que está no ar: sempre pede o "sim".
	return Resposta{Texto: "Parar " + nome + "?", Acao: &Acao{Tipo: AcaoParar, ExecucaoID: e.ID, Feito: "Parei " + nome + "."}, Confirmar: true}
}

func nomes(a []Achado) []string {
	r := make([]string, len(a))
	for i, x := range a {
		r[i] = x.Nome
	}
	return r
}

// lista junta com vírgulas e "e" no fim; mais de quatro vira "e mais N".
func lista(itens []string) string {
	if len(itens) > 4 {
		resto := len(itens) - 3
		itens = append(itens[:3:3], fmt.Sprintf("mais %s", numero(resto, false)))
	}
	switch len(itens) {
	case 0:
		return ""
	case 1:
		return itens[0]
	}
	// Com vírgula dentro de um item, os itens se separam por ponto e vírgula.
	sep, fim := ", ", " e "
	for _, i := range itens {
		if strings.Contains(i, ",") {
			sep, fim = "; ", "; e "
		}
	}
	return strings.Join(itens[:len(itens)-1], sep) + fim + itens[len(itens)-1]
}

// numero por extenso até dez (fala melhor); daí para cima, em algarismos.
func numero(n int, feminino bool) string {
	nomes := []string{"zero", "um", "dois", "três", "quatro", "cinco", "seis", "sete", "oito", "nove", "dez"}
	if n < 0 || n >= len(nomes) {
		return fmt.Sprint(n)
	}
	if feminino && n == 1 {
		return "uma"
	}
	if feminino && n == 2 {
		return "duas"
	}
	return nomes[n]
}

func contar(n int, um, varios string) string {
	if n == 1 {
		return um
	}
	return capitalizar(numero(n, false) + " " + varios)
}

func capitalizar(s string) string {
	for i, r := range s {
		return strings.ToUpper(string(r)) + s[i+len(string(r)):]
	}
	return s
}
