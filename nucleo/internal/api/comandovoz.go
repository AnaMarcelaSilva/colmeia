package api

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"net/http"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/comandovoz"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
)

// Os comandos falados à abelha: a tela (ou a abelha fixa) manda a frase que
// o colmeia-voz transcreveu, o interpretador (pacote comandovoz) entende e
// monta a resposta com o que o núcleo já sabe, e aqui a ação acontece. O que
// mexe no quadro ou derruba algo fica guardado até o "sim" (falado ou no
// clique), por pouco tempo e só para o perfil que perguntou.

// Uma frase maior que isto não é um comando.
const limiteFrase = 500

// Uma pergunta sem resposta por este tempo deixa de valer.
const validadeConfirmacao = time.Minute

type pendente struct {
	ficha  string
	acao   comandovoz.Acao
	expira time.Time
}

// confirmacoes guarda a última pergunta de cada perfil (uma nova troca a anterior).
type confirmacoes struct {
	mu     sync.Mutex
	perfil map[int64]pendente
}

func (c *confirmacoes) guardar(perfil int64, a comandovoz.Acao) string {
	b := make([]byte, 16)
	rand.Read(b)
	ficha := hex.EncodeToString(b)
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.perfil == nil {
		c.perfil = map[int64]pendente{}
	}
	c.perfil[perfil] = pendente{ficha: ficha, acao: a, expira: time.Now().Add(validadeConfirmacao)}
	return ficha
}

// tem diz se há uma pergunta valendo para o perfil.
func (c *confirmacoes) tem(perfil int64) bool {
	c.mu.Lock()
	defer c.mu.Unlock()
	p, ok := c.perfil[perfil]
	return ok && time.Now().Before(p.expira)
}

// tirar devolve a pergunta do perfil (e a esquece). Com ficha, só se for
// aquela; sem ficha, a que estiver no ar ("sim" falado).
func (c *confirmacoes) tirar(perfil int64, ficha string) (comandovoz.Acao, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	p, ok := c.perfil[perfil]
	if !ok || (ficha != "" && ficha != p.ficha) {
		return comandovoz.Acao{}, false
	}
	delete(c.perfil, perfil)
	return p.acao, time.Now().Before(p.expira)
}

// respostaVoz é o que a tela mostra no balão (e fala, com a voz da abelha).
type respostaVoz struct {
	Texto string `json:"texto"`
	// Ficha da pergunta que espera "sim" (clique em Sim ou fala).
	Confirmar string `json:"confirmar,omitempty"`
	// A tarefa que a tela deve abrir e, com Ditar, o agente em cuja caixa o
	// ditado liga ("responder a um").
	Abrir  int64 `json:"abrir,omitempty"`
	Agente int64 `json:"agente,omitempty"`
	Ditar  bool  `json:"ditar,omitempty"`
	// A lista aberta (a ficha vai no próximo comando) e a página para o balão.
	Sessao string    `json:"sessao,omitempty"`
	Lista  *listaVoz `json:"lista,omitempty"`
	// Ouvir: depois de mostrar (e falar) a resposta, o microfone abre de
	// novo, com este vocabulário para o Whisper (vazio: o do perfil).
	Ouvir  bool   `json:"ouvir,omitempty"`
	Prompt string `json:"prompt,omitempty"`
}

func (s *Servidor) rotasComandoVoz(mux *http.ServeMux) {
	mux.HandleFunc("POST /v1/perfis/{id}/voz/comando", s.comandoFalado)
	mux.HandleFunc("POST /v1/perfis/{id}/voz/confirmar", s.confirmarFalado)
	s.rotasListaVoz(mux)
}

func (s *Servidor) comandoFalado(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Texto string `json:"texto"`
		// A ficha da lista aberta: o número dito vale para ela.
		Lista string `json:"lista"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if utf8.RuneCountInString(pedido.Texto) > limiteFrase {
		responderErro(w, dados.ErrInvalido{Motivo: "Frase longa demais para um comando."})
		return
	}
	perfil, err := s.Banco.Perfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Escrita no resumo da abelha, a frase chega também só com a pergunta livre ligada.
	if !perfil.Comandos && !perfil.PerguntaIA {
		responderErro(w, dados.ErrInvalido{Motivo: "Os comandos falados estão desligados. Ligue em Ajustes › Voz."})
		return
	}
	if pedido.Lista != "" {
		sessao, ok := s.listas.pegar(id, pedido.Lista)
		if !ok {
			responderJSON(w, respostaVoz{Texto: "Essa lista já não vale. Me chame de novo para ouvir a lista."})
			return
		}
		if resposta, daLista := s.comandoDaLista(r.Context(), perfil, sessao, pedido.Texto); daLista {
			responderJSON(w, resposta)
			return
		}
	}
	p := comandovoz.Entender(pedido.Texto, perfil.MascoteNome)
	switch p.Intencao {
	case comandovoz.SoNome:
		// Só o nome: a abelha fala a lista do que importa agora.
		if perfil.Comandos {
			resposta, err := s.abrirLista(r.Context(), id)
			if err != nil {
				responderErro(w, err)
				return
			}
			responderJSON(w, resposta)
			return
		}
	case comandovoz.Sim:
		a, valida := s.confirmacoes.tirar(id, "")
		if !valida {
			responderJSON(w, respostaVoz{Texto: "Não tenho nada esperando um sim agora."})
			return
		}
		responderJSON(w, s.executarFalado(r.Context(), id, a))
		return
	case comandovoz.Nao:
		s.confirmacoes.tirar(id, "")
		responderJSON(w, respostaVoz{Texto: "Tudo bem, não fiz nada."})
		return
	}
	// Não é um comando fixo: com a opção ligada, vira pergunta livre (e só então usa tokens).
	if p.Intencao == comandovoz.Desconhecida && perfil.PerguntaIA {
		responderJSON(w, s.perguntaLivre(r.Context(), perfil, comandovoz.SemChamado(pedido.Texto, perfil.MascoteNome)))
		return
	}
	q, err := s.quadroFalado(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta := comandovoz.Responder(p, q)
	switch {
	case resposta.Acao == nil:
		responderJSON(w, respostaVoz{Texto: resposta.Texto, Ouvir: resposta.Ouvir})
	case resposta.Confirmar:
		// O microfone abre de novo para o "sim" falado.
		responderJSON(w, respostaVoz{Texto: resposta.Texto, Confirmar: s.confirmacoes.guardar(id, *resposta.Acao), Ouvir: true, Prompt: vocabularioConfirmar})
	default:
		responderJSON(w, s.executarFalado(r.Context(), id, *resposta.Acao))
	}
}

// confirmarFalado é o Sim (ou o Não) clicado no balão.
func (s *Servidor) confirmarFalado(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Ficha string `json:"ficha"`
		Sim   bool   `json:"sim"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Ficha == "" {
		responderErro(w, dados.ErrInvalido{Motivo: "Falta a ficha da pergunta."})
		return
	}
	a, valida := s.confirmacoes.tirar(id, pedido.Ficha)
	switch {
	case !pedido.Sim:
		responderJSON(w, respostaVoz{Texto: "Tudo bem, não fiz nada."})
	case !valida:
		responderJSON(w, respostaVoz{Texto: "Essa pergunta já não vale. Peça de novo."})
	default:
		responderJSON(w, s.executarFalado(r.Context(), id, a))
	}
}

// executarFalado faz a ação (já confirmada, se precisava) e diz como foi.
// Tudo é conferido de novo aqui: a configuração e a tarefa têm de ser do perfil.
func (s *Servidor) executarFalado(ctx context.Context, perfil int64, a comandovoz.Acao) respostaVoz {
	falhou := func(err error) respostaVoz { return respostaVoz{Texto: "Não consegui: " + mensagemDeErro(err)} }
	switch a.Tipo {
	case comandovoz.AcaoRodar, comandovoz.AcaoParar:
		_, dono, err := s.Banco.ComandoPorID(ctx, a.ExecucaoID)
		if err == nil && dono != perfil {
			err = dados.ErrNaoEncontrado
		}
		if err != nil {
			return falhou(err)
		}
		if a.Tipo == comandovoz.AcaoParar {
			// Como o Parar da tela: o programa tem uns segundos para sair.
			go s.Execucoes.Parar(a.ExecucaoID)
		} else if _, err := s.rodarConfiguracao(ctx, a.ExecucaoID, pedidoRodar{}); err != nil {
			return falhou(err)
		}
		return respostaVoz{Texto: a.Feito}
	case comandovoz.AcaoEncerrar, comandovoz.AcaoAbrir, comandovoz.AcaoRevisao, comandovoz.AcaoResponder:
		_, _, dono, err := s.Banco.Tarefa(ctx, a.TarefaID)
		if err == nil && dono != perfil {
			err = dados.ErrNaoEncontrado
		}
		if err != nil {
			return falhou(err)
		}
		switch a.Tipo {
		case comandovoz.AcaoAbrir:
			return respostaVoz{Texto: a.Feito, Abrir: a.TarefaID}
		case comandovoz.AcaoResponder:
			ag, err := s.Banco.Agente(ctx, a.AgenteID)
			if err == nil && ag.TarefaID != a.TarefaID {
				err = dados.ErrNaoEncontrado
			}
			if err != nil {
				return falhou(err)
			}
			return respostaVoz{Texto: a.Feito, Abrir: a.TarefaID, Agente: a.AgenteID, Ditar: true}
		}
		coluna := "concluido"
		if a.Tipo == comandovoz.AcaoRevisao {
			coluna = "revisao"
		}
		if _, err := s.Banco.AtualizarTarefa(ctx, a.TarefaID, dados.Mudanca{Coluna: &coluna}, dados.OrigemVoce); err != nil {
			return falhou(err)
		}
		return respostaVoz{Texto: a.Feito}
	}
	return respostaVoz{Texto: comandovoz.Ajuda}
}

// quadroFalado é o retrato do perfil para o interpretador: os projetos com
// as configurações do Play, as tarefas e os agentes com o estado de agora.
func (s *Servidor) quadroFalado(ctx context.Context, perfil int64) (comandovoz.Quadro, error) {
	var q comandovoz.Quadro
	projetos, err := s.Banco.ListarProjetos(ctx, perfil)
	if err != nil {
		return q, err
	}
	for _, p := range projetos {
		cmds, err := s.Banco.ListarComandos(ctx, p.ID)
		if err != nil {
			return q, err
		}
		qp := comandovoz.Projeto{ID: p.ID, Nome: p.Nome}
		for _, c := range cmds {
			qp.Execucoes = append(qp.Execucoes, comandovoz.Execucao{ID: c.ID, Nome: c.Nome, Rodando: s.Execucoes.Ativa(c.ID)})
		}
		q.Projetos = append(q.Projetos, qp)
	}
	tarefas, err := s.Banco.ListarTarefasDoPerfil(ctx, perfil)
	if err != nil {
		return q, err
	}
	for _, t := range tarefas {
		q.Tarefas = append(q.Tarefas, comandovoz.Tarefa{ID: t.ID, ProjetoID: t.ProjetoID, Titulo: t.Titulo, Coluna: t.Coluna})
	}
	agentes, err := s.Banco.ListarAgentesDoPerfil(ctx, perfil)
	if err != nil {
		return q, err
	}
	fins, err := s.Banco.UltimosFins(ctx, perfil)
	if err != nil {
		return q, err
	}
	for _, a := range agentes {
		e := s.comEstado(a, "", fins)
		q.Agentes = append(q.Agentes, comandovoz.Agente{
			ID: a.ID, TarefaID: a.TarefaID, Nome: ferramentas.Nome(a.Ferramenta),
			Ativo: e.Ativo, Estado: e.Estado, Motivo: e.Motivo, Erro: !e.Ativo && e.UltimoFim != nil && e.UltimoFim.Erro,
		})
	}
	return q, nil
}
