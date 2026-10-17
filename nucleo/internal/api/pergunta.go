package api

import (
	"context"
	"os/exec"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/comandovoz"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/pergunta"
)

// A pergunta livre com IA: o que a abelha não reconheceu como comando fixo,
// com a opção ligada no perfil, vai num pedido só ao Claude Code (o mesmo
// mecanismo da sprint gerada) com um retrato curto e de leitura do perfil.
// Nada aqui é gravado: nem a pergunta, nem o retrato, nem a resposta, e não
// há evento.

// perguntando: uma pergunta por perfil de cada vez (cada uma gasta tokens).
type perguntando struct {
	mu     sync.Mutex
	perfil map[int64]bool
}

func (p *perguntando) comecar(perfil int64) bool {
	p.mu.Lock()
	defer p.mu.Unlock()
	if p.perfil[perfil] {
		return false
	}
	if p.perfil == nil {
		p.perfil = map[int64]bool{}
	}
	p.perfil[perfil] = true
	return true
}

func (p *perguntando) terminar(perfil int64) {
	p.mu.Lock()
	delete(p.perfil, perfil)
	p.mu.Unlock()
}

// Só estes tipos da linha do tempo vão no retrato: o andamento das tarefas e
// dos agentes. Ficam de fora os bancos, o navegador (endereços), a lousa, as
// capturas e os pedidos ao agente (o texto que você escreveu para ele).
var tiposDoRetrato = map[string]bool{
	linha.TipoConcluiu: true, linha.TipoCriou: true, linha.TipoMoveu: true, linha.TipoRemoveu: true,
	linha.TipoSessao: true, linha.TipoSessaoAberta: true, linha.TipoSessaoAguardando: true, linha.TipoSessaoParada: true,
	linha.TipoErro: true, linha.TipoInterrompido: true, linha.TipoProjeto: true, linha.TipoNota: true, linha.TipoNotaAgente: true,
}

var estadosDaColuna = map[string]string{
	"backlog":     "no backlog",
	"trabalhando": "com o agente trabalhando",
	"aguardando":  "aguardando você",
	"revisao":     "em revisão",
	"concluido":   "concluída",
}

// contaDoClaude é o ambiente com a conta que os agentes do perfil usam.
func (s *Servidor) contaDoClaude(ctx context.Context, perfil int64) ([]string, error) {
	separada, err := s.pastaSeparada(ctx, perfil, claudecode.Executavel)
	if err != nil || separada == "" {
		return nil, err
	}
	return []string{ferramentas.Variavel(claudecode.Executavel) + "=" + separada}, nil
}

// perguntaLivre responde no balão (e na voz) o que não foi um comando fixo.
// O que dá errado também vira frase do balão, não erro HTTP: a abelha explica.
func (s *Servidor) perguntaLivre(ctx context.Context, perfil dados.Perfil, texto string) respostaVoz {
	texto, err := pergunta.Conferir(texto)
	if err != nil {
		return respostaVoz{Texto: mensagemDeErro(err)}
	}
	executavel, err := exec.LookPath(claudecode.Executavel)
	if err != nil {
		return respostaVoz{Texto: "Para responder perguntas livres eu preciso do Claude Code instalado, e não o achei. Os comandos fixos continuam valendo."}
	}
	if !s.perguntando.comecar(perfil.ID) {
		return respostaVoz{Texto: "Ainda estou pensando na pergunta anterior. Espere a resposta."}
	}
	defer s.perguntando.terminar(perfil.ID)
	env, err := s.contaDoClaude(ctx, perfil.ID)
	if err != nil {
		return respostaVoz{Texto: "Não consegui responder: " + mensagemDeErro(err)}
	}
	retrato, err := s.retratoDoPerfil(ctx, perfil)
	if err != nil {
		return respostaVoz{Texto: "Não consegui responder: " + mensagemDeErro(err)}
	}
	resposta, err := pergunta.Perguntador{Executavel: executavel, Env: env}.Responder(ctx, texto, retrato)
	if err != nil {
		return respostaVoz{Texto: "Não consegui responder: " + mensagemDeErro(err)}
	}
	return respostaVoz{Texto: resposta}
}

// retratoDoPerfil é o que o agente sabe: as tarefas com o estado, os agentes
// (nome, estado e o motivo de uma lista fixa), as últimas frases da reunião
// e o que aconteceu hoje, em português. Nada de terminal, pasta, conexão de
// banco ou mensagem ao agente; textos com cara de segredo ficam de fora.
func (s *Servidor) retratoDoPerfil(ctx context.Context, perfil dados.Perfil) (pergunta.Contexto, error) {
	agora := time.Now()
	c := pergunta.Contexto{Agora: agora.Format("02/01/2006 15:04"), Tarefas: []pergunta.Tarefa{}, Hoje: []pergunta.Evento{}}
	q, err := s.quadroFalado(ctx, perfil.ID)
	if err != nil {
		return c, err
	}
	projetos := map[int64]string{}
	for _, p := range q.Projetos {
		projetos[p.ID] = p.Nome
	}
	// Da mais nova para a mais antiga.
	ids := make([]int64, 0, len(q.Tarefas))
	for i := len(q.Tarefas) - 1; i >= 0; i-- {
		ids = append(ids, q.Tarefas[i].ID)
	}
	daily, err := s.Banco.UltimasNotas(ctx, ids, "daily")
	if err != nil {
		return c, err
	}
	sprint, err := s.Banco.UltimasNotas(ctx, ids, "sprint")
	if err != nil {
		return c, err
	}
	// As abertas primeiro: com o limite, as concluídas mais antigas saem.
	var abertas, concluidas []pergunta.Tarefa
	for i := len(q.Tarefas) - 1; i >= 0; i-- {
		t := q.Tarefas[i]
		pt := pergunta.Tarefa{
			Titulo: pergunta.Limpo(t.Titulo), Projeto: projetos[t.ProjetoID], Estado: estadosDaColuna[t.Coluna],
			FraseDaily: pergunta.Limpo(daily[t.ID].Texto), FraseSprint: pergunta.Limpo(sprint[t.ID].Texto),
		}
		for _, a := range q.Agentes {
			if a.TarefaID == t.ID {
				pt.Agentes = append(pt.Agentes, agenteDoRetrato(a))
			}
		}
		if t.Coluna == "concluido" {
			concluidas = append(concluidas, pt)
		} else {
			abertas = append(abertas, pt)
		}
	}
	todas := append(abertas, concluidas...)
	c.Tarefas = todas[:min(len(todas), pergunta.MaxTarefas)]
	c.Hoje, err = s.hojeDoPerfil(ctx, perfil, agora)
	return c, err
}

func agenteDoRetrato(a comandovoz.Agente) pergunta.Agente {
	estado := "parado"
	switch {
	case a.Ativo && a.Estado == "aguardando":
		estado = "esperando você"
		if a.Motivo != "" {
			estado += ": " + a.Motivo
		}
	case a.Ativo:
		estado = "trabalhando"
	}
	return pergunta.Agente{Nome: a.Nome, Estado: estado, Erro: !a.Ativo && a.Erro}
}

// hojeDoPerfil são as linhas de hoje da linha do tempo, só dos tipos do
// retrato, as mais novas, em ordem de hora.
func (s *Servidor) hojeDoPerfil(ctx context.Context, perfil dados.Perfil, agora time.Time) ([]pergunta.Evento, error) {
	hoje := []pergunta.Evento{}
	lc, err := s.contextoDaLinha(ctx, perfil, recorte{})
	if err != nil {
		return hoje, err
	}
	inicio := time.Date(agora.Year(), agora.Month(), agora.Day(), 0, 0, 0, 0, time.Local)
	eventos, err := s.Banco.ListarEventos(ctx, dados.FiltroEventos{Perfil: perfil.ID, Desde: inicioDoDia(inicio), Limite: limiteMaximoLinha})
	if err != nil {
		return hoje, err
	}
	for _, dia := range linha.Montar(eventos, lc) {
		for _, it := range dia.Itens {
			if !tiposDoRetrato[it.Tipo] {
				continue
			}
			if t := pergunta.Limpo(it.Texto); t != "" {
				hoje = append(hoje, pergunta.Evento{Hora: it.Hora, Texto: t})
			}
		}
	}
	// A linha do tempo vem da mais nova para a mais antiga.
	if len(hoje) > pergunta.MaxEventos {
		hoje = hoje[:pergunta.MaxEventos]
	}
	for i, j := 0, len(hoje)-1; i < j; i, j = i+1, j-1 {
		hoje[i], hoje[j] = hoje[j], hoje[i]
	}
	return hoje, nil
}
