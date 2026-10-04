package api

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"net/http"
	"sort"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/bancos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

// Consultas dos agentes ao banco: só leitura, sempre com a sua aprovação na
// tela. O pedido fica em memória (não na corrente de eventos, porque leva o
// SQL), o cartão do agente fica em "Aguardando você" enquanto isso, e sem
// resposta no prazo conta como recusa.

const (
	// PrazoAprovacaoPadrao: depois disso sem resposta, o pedido expira.
	PrazoAprovacaoPadrao = 5 * time.Minute
	// O que o agente pode receber de uma consulta.
	maxLinhasAgente = 200
	maxTextoAgente  = 64 << 10
	tempoAgente     = 30 * time.Second
	maxMotivo       = 500
)

// Resultados de um pedido do agente.
const (
	aprovada  = "aprovada"
	recusada  = "recusada"
	expirou   = "expirou"
	cancelada = "cancelada"
)

type respostaAprovacao struct {
	aprovar bool
	motivo  string
}

// aprovacao é um pedido de consulta do agente esperando você.
type aprovacao struct {
	ID           string `json:"id"`
	ConexaoID    int64  `json:"conexao_id"`
	Conexao      string `json:"conexao"`
	Tipo         string `json:"tipo"`
	AgenteID     int64  `json:"agente_id"`
	TarefaID     int64  `json:"tarefa_id"`
	Agente       string `json:"agente"`
	Tarefa       string `json:"tarefa"`
	SQL          string `json:"sql"`
	Banco        string `json:"banco"`
	Limite       int    `json:"limite"`
	CriadaHora   string `json:"criada_hora"`
	Expira       string `json:"expira"`
	ExpiraHora   string `json:"expira_hora"`
	PrecisaSenha bool   `json:"precisa_senha"`

	perfil   int64
	resposta chan respostaAprovacao
}

func (s *Servidor) rotasBancosDoAgente(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/agente/bancos", s.doAgente(s.agenteListarBancos))
	mux.HandleFunc("POST /v1/agente/bancos/{id}/consultas", s.doAgente(s.agenteConsultar))
}

func (s *Servidor) agenteListarBancos(w http.ResponseWriter, r *http.Request, q quemAgente) {
	lista, err := s.Banco.ListarConexoes(r.Context(), q.perfil)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Sem host nem usuário: o agente só precisa saber o que pode pedir.
	conexoes := []map[string]any{}
	for _, c := range lista {
		if c.Agentes {
			conexoes = append(conexoes, map[string]any{"id": c.ID, "nome": c.Nome, "tipo": c.Tipo, "banco": c.Banco})
		}
	}
	responderJSON(w, map[string]any{"conexoes": conexoes})
}

func novoIDAprovacao() string {
	var b [12]byte
	rand.Read(b[:])
	return hex.EncodeToString(b[:])
}

func (a *aprovacao) mensagem() map[string]any {
	return map[string]any{"tipo": "banco.aprovacao", "acao": "pedida", "aprovacao": a, "agente_id": a.AgenteID, "tarefa_id": a.TarefaID}
}

// pendentes do perfil, da mais antiga para a mais nova.
func (s *Servidor) aprovacoesPendentes(perfil int64) []*aprovacao {
	s.muAprovacoes.Lock()
	defer s.muAprovacoes.Unlock()
	lista := []*aprovacao{}
	for _, a := range s.aprovacoes {
		if a.perfil == perfil {
			lista = append(lista, a)
		}
	}
	sort.Slice(lista, func(i, j int) bool { return lista[i].Expira < lista[j].Expira })
	return lista
}

func (s *Servidor) listarAprovacoes(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"aprovacoes": s.aprovacoesPendentes(id)})
}

// responderAprovacao: você aprova ou recusa (com um motivo opcional, que
// vai só para o agente). Sem senha na memória, a aprovação traz a senha.
func (s *Servidor) responderAprovacao(w http.ResponseWriter, r *http.Request) {
	id := r.PathValue("id")
	var pedido struct {
		Aprovar bool   `json:"aprovar"`
		Motivo  string `json:"motivo"`
		Senha   string `json:"senha"`
		Guardar bool   `json:"guardar"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if len([]rune(pedido.Motivo)) > maxMotivo || strings.ContainsFunc(pedido.Motivo, func(r rune) bool { return r < ' ' && r != '\n' }) {
		responderErro(w, dados.ErrInvalido{Motivo: "o motivo pode ter até 500 caracteres, sem caracteres de controle"})
		return
	}
	s.muAprovacoes.Lock()
	a := s.aprovacoes[id]
	s.muAprovacoes.Unlock()
	if a == nil {
		responderErro(w, dados.ErrInvalido{Motivo: "este pedido já foi resolvido ou expirou"})
		return
	}
	if pedido.Aprovar && pedido.Senha != "" {
		c, err := s.Banco.ConexaoBanco(r.Context(), a.ConexaoID)
		if err != nil {
			responderErro(w, err)
			return
		}
		// Senha errada não encerra o pedido: você digita de novo, e o
		// agente só recebe resposta quando a consulta roda, você recusa ou
		// o prazo acaba. A senha recusada não fica guardada.
		if _, err := bancos.Testar(r.Context(), config(c, pedido.Senha)); bancos.SenhaRecusada(err) {
			responderStatus(w, http.StatusUnprocessableEntity, map[string]any{"erro": "Usuário ou senha recusados · tente de novo.", "senha_recusada": true})
			return
		}
		if _, err := s.guardarSenha(r.Context(), c, pedido.Senha, pedido.Guardar); err != nil {
			responderErro(w, err)
			return
		}
	}
	select {
	case a.resposta <- respostaAprovacao{aprovar: pedido.Aprovar, motivo: strings.TrimSpace(pedido.Motivo)}:
	default:
		responderErro(w, dados.ErrInvalido{Motivo: "este pedido já foi respondido"})
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

// agenteConsultar: o agente pede uma consulta. Só leitura, numa conexão
// liberada para agentes, e só depois da sua aprovação.
func (s *Servidor) agenteConsultar(w http.ResponseWriter, r *http.Request, q quemAgente) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		SQL    string `json:"sql"`
		Banco  string `json:"banco"`
		Limite int    `json:"limite"`
	}
	if err := lerAte(r, &pedido, limiteSQL+4096); err != nil {
		responderErro(w, err)
		return
	}
	c, err := s.Banco.ConexaoBanco(r.Context(), id)
	if err != nil || c.PerfilID != q.perfil || !c.Agentes {
		responderErro(w, dados.ErrInvalido{Motivo: "conexão não liberada para agentes; use listar_bancos para ver as que estão"})
		return
	}
	if pedido.Limite <= 0 || pedido.Limite > maxLinhasAgente {
		pedido.Limite = maxLinhasAgente
	}
	if pedido.Banco == "" {
		pedido.Banco = c.Banco
	}
	if err := nomeDeBancoValido(pedido.Banco); err != nil {
		responderErro(w, err)
		return
	}
	cl, err := bancos.Classificar(c.Tipo, pedido.SQL)
	if err != nil {
		responderErro(w, dados.ErrInvalido{Motivo: err.Error()})
		return
	}
	if cl.Classe != bancos.Leitura {
		responderErro(w, dados.ErrInvalido{Motivo: "o agente só consulta: " + cl.Verbo + " altera dados e não é permitido ao agente"})
		return
	}
	if f := bancos.FuncaoNegadaAoAgente(c.Tipo, pedido.SQL); f != "" {
		responderErro(w, dados.ErrInvalido{Motivo: "a função " + f + " não é permitida ao agente"})
		return
	}
	agora := time.Now()
	_, faltaSenha := s.senhaDe(c)
	a := &aprovacao{ID: novoIDAprovacao(), ConexaoID: c.ID, Conexao: c.Nome, Tipo: c.Tipo, AgenteID: q.ctx.ID, TarefaID: q.tarefa.ID,
		Agente: ferramentas.Nome(q.ctx.Ferramenta) + " (" + q.ctx.Papel + ")", Tarefa: q.tarefa.Titulo, SQL: pedido.SQL, Banco: pedido.Banco,
		Limite: pedido.Limite, CriadaHora: hora(agora), Expira: agora.Add(s.PrazoAprovacao).UTC().Format(time.RFC3339Nano),
		ExpiraHora: hora(agora.Add(s.PrazoAprovacao)), PrecisaSenha: errors.Is(faltaSenha, errSemSenha), perfil: q.perfil,
		resposta: make(chan respostaAprovacao, 1)}
	s.muAprovacoes.Lock()
	s.aprovacoes[a.ID] = a
	s.muAprovacoes.Unlock()
	sessao, temSessao := s.Agentes.Pegar(q.ctx.ID)
	if temSessao {
		sessao.Segurar(terminal.AprovarConsulta)
	}
	s.Avisos.Publicar(q.perfil, a.mensagem())

	var resposta respostaAprovacao
	resultado := cancelada
	prazo := time.NewTimer(s.PrazoAprovacao)
	select {
	case resposta = <-a.resposta:
		resultado = recusada
		if resposta.aprovar {
			resultado = aprovada
		}
	case <-prazo.C:
		resultado = expirou
	case <-r.Context().Done():
	}
	prazo.Stop()
	s.muAprovacoes.Lock()
	delete(s.aprovacoes, a.ID)
	s.muAprovacoes.Unlock()
	if temSessao {
		sessao.Soltar()
	}
	feita := consultaFeita{sql: pedido.SQL, banco: pedido.Banco, verbo: cl.Verbo, agente: q.ctx, resultado: resultado}
	resolvida := map[string]any{"tipo": "banco.aprovacao", "acao": "resolvida", "aprovacao_id": a.ID, "agente_id": a.AgenteID,
		"tarefa_id": a.TarefaID, "conexao_id": c.ID, "resultado": resultado, "hora": hora(time.Now())}
	if resultado != aprovada {
		s.registrarConsulta(r.Context(), c, feita)
		s.Avisos.Publicar(q.perfil, resolvida)
		switch resultado {
		case recusada:
			texto := "O usuário recusou a consulta."
			if resposta.motivo != "" {
				texto += " Motivo: " + resposta.motivo
			}
			responderStatus(w, http.StatusConflict, map[string]any{"erro": texto, "resultado": resultado})
		case expirou:
			responderStatus(w, http.StatusConflict, map[string]any{"erro": "O usuário não respondeu em " + segundos(s.PrazoAprovacao) +
				"; a consulta não foi feita.", "resultado": resultado})
		default:
			responderStatus(w, http.StatusConflict, map[string]any{"erro": "O pedido foi cancelado.", "resultado": resultado})
		}
		return
	}
	// Aprovada: lê a conexão de novo (pode ter mudado enquanto esperava).
	ctx := context.WithoutCancel(r.Context())
	c, err = s.Banco.ConexaoBanco(ctx, id)
	var cfg bancos.Config
	if err == nil && !c.Agentes {
		err = dados.ErrInvalido{Motivo: "a conexão deixou de aceitar agentes"}
	}
	if err == nil {
		var senha string
		if senha, err = s.senhaDe(c); err == nil {
			cfg = config(c, senha)
			// Mesmo numa conexão com escrita, o agente só lê.
			cfg.Escrita = false
		}
	}
	var r2 bancos.Resultado
	if err == nil {
		r2, err = s.Bancos.Executar(r.Context(), cfg, bancos.Opcoes{SQL: pedido.SQL, Banco: pedido.Banco, Limite: pedido.Limite, Tempo: tempoAgente, SemCursor: true})
	}
	feita.linhas, feita.ms, feita.erro = int64(len(r2.Linhas)), r2.Ms, err != nil
	s.registrarConsulta(ctx, c, feita)
	resolvida["linhas"], resolvida["ms"] = feita.linhas, feita.ms
	if err != nil {
		resolvida["erro"] = true
		s.Avisos.Publicar(q.perfil, resolvida)
		if errors.Is(err, errSemSenha) {
			responderErro(w, dados.ErrInvalido{Motivo: "a conexão está sem senha na Colmeia; o usuário precisa digitá-la"})
			return
		}
		frase, detalhe := bancos.Explicar(err, cfg)
		var eb bancos.ErrBanco
		if errors.As(err, &eb) && frase == "Não conectou." {
			frase, detalhe = eb.Mensagem, ""
		}
		if detalhe != "" {
			frase += " (" + detalhe + ")"
		}
		responderErro(w, dados.ErrInvalido{Motivo: frase})
		return
	}
	s.Avisos.Publicar(q.perfil, resolvida)
	texto, cortado := textoTabela(r2, maxTextoAgente)
	cabecalho := fmt.Sprintf("%d linhas · %d ms", len(r2.Linhas), r2.Ms)
	if r2.Mais {
		cabecalho += fmt.Sprintf(" · há mais (limite de %d linhas)", pedido.Limite)
	}
	if cortado {
		cabecalho += " · cortado em 64 KB"
	}
	responderJSON(w, map[string]any{"texto": cabecalho + "\n" + texto, "linhas": len(r2.Linhas), "mais": r2.Mais})
}

// cancelarAprovacoes resolve os pedidos pendentes (o núcleo vai desligar).
func (s *Servidor) cancelarAprovacoes() {
	s.muAprovacoes.Lock()
	defer s.muAprovacoes.Unlock()
	for _, a := range s.aprovacoes {
		select {
		case a.resposta <- respostaAprovacao{}:
		default:
		}
	}
}
