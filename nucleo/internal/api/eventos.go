package api

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"time"

	"github.com/coder/websocket"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/avisos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

// A tela não manda nada pelo WebSocket de eventos além do fechamento.
const limiteMensagemEventos = 4 << 10

// Mudanças de estado guardadas para o trabalhador antes de serem descartadas.
const filaMudancas = 1024

// hora local curta, para a tela mostrar "desde 14:32" sem lidar com fuso.
func hora(t time.Time) string { return t.Local().Format("15:04") }

func horaDoMomento(momento string) string {
	t, err := time.Parse(time.RFC3339Nano, momento)
	if err != nil {
		return ""
	}
	return hora(t)
}

// fimAgente é como um agente terminou, do jeito que a tela mostra.
type fimAgente struct {
	Codigo int    `json:"codigo"`
	Erro   bool   `json:"erro"`
	Motivo string `json:"motivo"`
	Em     string `json:"em"`
	Hora   string `json:"hora"`
	Texto  string `json:"texto"`
}

// classificar decide se o fim de um agente foi erro. Só é erro o que é erro
// de fato: o terminal comum sai com o código do último comando, e Ctrl+C
// (130) é você encerrando.
func classificar(ferramenta string, saida terminal.Saida, pelaColmeia string) (motivo string, erro bool) {
	switch {
	case pelaColmeia != "":
		return pelaColmeia, false
	case ferramenta == "shell", saida.Codigo == 0, saida.Codigo == 130:
		return "terminou", false
	case saida.PorSinal:
		return "interrompido", false
	default:
		return "erro", true
	}
}

func textoDoFim(ferramenta, papel, motivo string, codigo int) string {
	nome := ferramentas.Nome(ferramenta) + " (" + papel + ")"
	switch motivo {
	case "erro":
		return nome + " parou com erro (código " + itoa(codigo) + ")"
	case "interrompido":
		return nome + " foi interrompido"
	case terminal.PeloDesligar:
		return nome + " parou quando o núcleo foi encerrado"
	case terminal.PelaRemocao:
		return nome + " foi parado pela Colmeia"
	}
	if ferramenta == "shell" && codigo != 0 {
		return "Terminal encerrado (código " + itoa(codigo) + ")"
	}
	return nome + " terminou"
}

func itoa(n int) string {
	b, _ := json.Marshal(n)
	return string(b)
}

// Payload gravado em agente.terminou.
type terminouDados struct {
	Agente       int64  `json:"agente"`
	Ferramenta   string `json:"ferramenta"`
	Papel        string `json:"papel"`
	Tarefa       int64  `json:"tarefa"`
	Titulo       string `json:"titulo"`
	ProjetoNome  string `json:"projeto_nome"`
	Codigo       int    `json:"codigo"`
	PorSinal     bool   `json:"por_sinal"`
	Motivo       string `json:"motivo"`
	Erro         bool   `json:"erro"`
	DuracaoS     int64  `json:"duracao_s"`
	TrabalhandoS int64  `json:"trabalhando_s"`
	AguardandoS  int64  `json:"aguardando_s"`
}

func fimDoEvento(e dados.Evento) fimAgente {
	var d terminouDados
	json.Unmarshal(e.Dados, &d)
	return fimAgente{Codigo: d.Codigo, Erro: d.Erro, Motivo: d.Motivo, Em: e.Momento, Hora: horaDoMomento(e.Momento),
		Texto: textoDoFim(d.Ferramenta, d.Papel, d.Motivo, d.Codigo)}
}

// Trabalhador das mudanças dos agentes: grava os fins, aplica as regras de
// coluna e publica o estado. Fica fora da leitura dos terminais, que nunca
// espera pelo banco.

type mudancaAgente struct {
	id int64
	m  terminal.Mudanca
}

func (s *Servidor) iniciarTrabalhador() {
	s.mudancas = make(chan mudancaAgente, filaMudancas)
	s.parado = make(chan struct{})
	s.trabalhou = make(chan struct{})
	s.contextos = map[int64]dados.ContextoAgente{}
	s.Agentes.AoMudar = s.aoMudarAgente
	go s.trabalhar()
}

// aoMudarAgente nunca bloqueia a leitura por causa de um estado: se a fila
// encher, o estado se perde (o próximo corrige). O fim espera sua vez.
func (s *Servidor) aoMudarAgente(id int64, m terminal.Mudanca) {
	switch m.Tipo {
	case "estado":
		select {
		case s.mudancas <- mudancaAgente{id, m}:
		default:
			log.Printf("agente %d: estado descartado, fila cheia", id)
		}
	case "terminou":
		select {
		case s.mudancas <- mudancaAgente{id, m}:
		case <-s.parado:
		}
	}
}

func (s *Servidor) trabalhar() {
	defer close(s.trabalhou)
	for {
		select {
		case m := <-s.mudancas:
			s.tratarMudanca(m)
		case <-s.parado:
			for {
				select {
				case m := <-s.mudancas:
					s.tratarMudanca(m)
				default:
					return
				}
			}
		}
	}
}

// Encerrar grava o que ainda falta (o fim dos agentes) antes de o banco fechar.
func (s *Servidor) Encerrar() {
	if s.parado == nil {
		return
	}
	if s.Bancos != nil {
		s.cancelarAprovacoes()
		s.Bancos.FecharTodas()
	}
	close(s.parado)
	<-s.trabalhou
}

func (s *Servidor) guardarContexto(c dados.ContextoAgente) {
	s.muContextos.Lock()
	defer s.muContextos.Unlock()
	s.contextos[c.ID] = c
}

func (s *Servidor) contexto(id int64) (dados.ContextoAgente, bool) {
	s.muContextos.Lock()
	defer s.muContextos.Unlock()
	c, ok := s.contextos[id]
	return c, ok
}

func (s *Servidor) tratarMudanca(mu mudancaAgente) {
	c, ok := s.contexto(mu.id)
	if !ok {
		return
	}
	ctx := context.Background()
	m := mu.m
	switch m.Tipo {
	case "estado":
		if m.Estado == terminal.Aguardando && m.Motivo == terminal.EsperandoResposta {
			s.fecharRespondidos(ctx, c.ID, m.Desde)
		}
		s.Avisos.Publicar(c.Perfil, map[string]any{
			"tipo": "agente.estado", "agente_id": c.ID, "tarefa_id": c.TarefaID,
			"estado": m.Estado, "motivo": m.Motivo, "desde": m.Desde.UTC().Format(time.RFC3339Nano), "desde_hora": hora(m.Desde),
		})
		s.regraDeColuna(ctx, c, m.Estado)
		if m.Estado == terminal.Aguardando && m.Motivo == terminal.EsperandoResposta {
			s.tentarEntregar(c.ID)
		}
	case "terminou":
		motivo, erro := classificar(c.Ferramenta, m.Saida, m.PelaColmeia)
		d := terminouDados{
			Agente: c.ID, Ferramenta: c.Ferramenta, Papel: c.Papel, Tarefa: c.TarefaID, Titulo: c.Titulo, ProjetoNome: c.ProjetoNome,
			Codigo: m.Saida.Codigo, PorSinal: m.Saida.PorSinal, Motivo: motivo, Erro: erro,
			DuracaoS: int64(m.Duracao.Seconds()), TrabalhandoS: int64(m.Trabalhando.Seconds()), AguardandoS: int64(m.Aguardando.Seconds()),
		}
		if err := s.Banco.Registrar(ctx, "agente.terminou", c.Escopo(), d); err != nil {
			log.Printf("agente %d: gravando o fim: %v", c.ID, err)
		}
		// O token do agente morre com ele; o que ele não respondeu, falhou.
		// (Se ele já foi iniciado de novo, o token e os pedidos são da sessão nova.)
		if !s.Agentes.Ativa(c.ID) {
			s.limparMCP(c.ID)
			s.falharPedidosDoAgente(ctx, c.ID, motivoDaFalha(motivo))
		}
	}
}

// regraDeColuna move o cartão sozinho entre Trabalhando e Aguardando. Só a
// volta de uma mudança que o próprio núcleo fez: se você moveu o cartão, ele
// fica onde você pôs. Nada vai para Revisão ou Concluído sozinho.
func (s *Servidor) regraDeColuna(ctx context.Context, c dados.ContextoAgente, estado string) {
	t, _, _, err := s.Banco.Tarefa(ctx, c.TarefaID)
	if err != nil {
		return
	}
	var destino string
	switch {
	case estado == terminal.Aguardando && t.Coluna == "trabalhando":
		destino = "aguardando"
	case estado == terminal.Trabalhando && t.Coluna == "aguardando" && t.ColunaAuto && !s.alguemAguardando(t.ID):
		destino = "trabalhando"
	default:
		return
	}
	if _, err := s.Banco.AtualizarTarefa(ctx, t.ID, dados.Mudanca{Coluna: &destino}, dados.OrigemAutomatica); err != nil && !errors.Is(err, dados.ErrNaoEncontrado) {
		log.Printf("tarefa %d: mudança automática de coluna: %v", t.ID, err)
	}
}

// alguemAguardando diz se algum agente rodando na tarefa ainda espera você.
func (s *Servidor) alguemAguardando(tarefa int64) bool {
	for _, sessao := range s.Agentes.Sessoes() {
		if c, ok := s.contexto(sessao.ID); ok && c.TarefaID == tarefa && !sessao.Encerrada() {
			if estado, _, _ := sessao.Estado(); estado == terminal.Aguardando {
				return true
			}
		}
	}
	return false
}

// publicarEventos leva às telas cada evento gravado, já no formato que a tela
// aplica: com o objeto inteiro, para aplicar duas vezes não mudar nada.
func (s *Servidor) publicarEventos(eventos []dados.Evento) {
	for _, e := range eventos {
		if m := s.mensagemDoEvento(e); m != nil {
			m["evento"] = e.ID
			s.Avisos.Publicar(e.Escopo.Perfil, m)
		}
	}
}

func (s *Servidor) mensagemDoEvento(e dados.Evento) map[string]any {
	ler := func(v any) { json.Unmarshal(e.Dados, v) }
	switch e.Tipo {
	case "projeto.criado":
		var p dados.Projeto
		ler(&p)
		return map[string]any{"tipo": e.Tipo, "projeto": p}
	case "projeto.removido":
		return map[string]any{"tipo": e.Tipo, "projeto_id": e.Escopo.Projeto}
	case "tarefa.criada":
		var t dados.Tarefa
		ler(&t)
		return map[string]any{"tipo": e.Tipo, "tarefa": t, "origem": dados.OrigemVoce}
	case "tarefa.atualizada", "tarefa.copia":
		var d struct {
			Tarefa dados.Tarefa `json:"tarefa"`
			Origem string       `json:"origem"`
		}
		ler(&d)
		return map[string]any{"tipo": "tarefa.atualizada", "tarefa": d.Tarefa, "origem": cmpOr(d.Origem, dados.OrigemVoce)}
	case "tarefa.removida":
		return map[string]any{"tipo": e.Tipo, "tarefa_id": e.Escopo.Tarefa, "projeto_id": e.Escopo.Projeto}
	case "agente.criado":
		var a dados.Agente
		ler(&a)
		return map[string]any{"tipo": e.Tipo, "agente": agenteComEstado{Agente: a}}
	case "agente.removido":
		return map[string]any{"tipo": e.Tipo, "agente_id": e.Escopo.Agente, "tarefa_id": e.Escopo.Tarefa}
	case "agente.iniciou":
		return map[string]any{"tipo": e.Tipo, "agente_id": e.Escopo.Agente, "tarefa_id": e.Escopo.Tarefa,
			"estado": terminal.Trabalhando, "desde": e.Momento, "desde_hora": horaDoMomento(e.Momento)}
	case "agente.terminou":
		return map[string]any{"tipo": e.Tipo, "agente_id": e.Escopo.Agente, "tarefa_id": e.Escopo.Tarefa, "fim": fimDoEvento(e)}
	case "anexo.adicionado":
		var d anexoDados
		ler(&d)
		m := map[string]any{"tipo": e.Tipo, "anexo_id": d.Anexo, "tarefa_id": e.Escopo.Tarefa, "agente_id": e.Escopo.Agente}
		if d.NaLousa {
			m["na_lousa"] = true
		}
		return m
	case "lousa.agente":
		// O evento não tem texto: os elementos (com o texto) são lidos agora.
		var d struct {
			Lousa     int64   `json:"lousa"`
			Elementos []int64 `json:"elementos"`
		}
		ler(&d)
		elementos, err := s.Banco.ElementosPorID(context.Background(), d.Elementos)
		if err != nil {
			log.Printf("lousa %d: lendo o que o agente acrescentou: %v", d.Lousa, err)
			elementos = []dados.Elemento{}
		}
		m := mensagemLousa(dados.Lousa{ID: d.Lousa, Dono: dados.DonoLousa{TarefaID: e.Escopo.Tarefa}}, elementos, nil)
		m["agente_id"] = e.Escopo.Agente
		return m
	case "anexo.removido":
		var d anexoDados
		ler(&d)
		return map[string]any{"tipo": e.Tipo, "anexo_id": d.Anexo, "tarefa_id": e.Escopo.Tarefa}
	case "nota.atualizada":
		var d notaDados
		ler(&d)
		m := map[string]any{"tipo": e.Tipo, "tarefa_id": e.Escopo.Tarefa, "nota_tipo": d.Tipo, "periodo": d.Periodo}
		if e.Escopo.Agente != 0 {
			m["agente_id"], m["modo"] = e.Escopo.Agente, d.Modo
		}
		return m
	case "pedido.criado", "pedido.entregue", "pedido.respondido", "pedido.cancelado", "pedido.falhou":
		var d struct {
			Pedido int64 `json:"pedido"`
		}
		ler(&d)
		m := map[string]any{"tipo": e.Tipo, "pedido_id": d.Pedido, "tarefa_id": e.Escopo.Tarefa, "agente_id": e.Escopo.Agente}
		// O pedido inteiro (com o texto, que não está no evento), lido agora.
		if p, err := s.Banco.Pedido(context.Background(), d.Pedido); err == nil {
			m["pedido"] = p
		}
		return m
	case "banco.conexao":
		var d struct {
			Acao      string `json:"acao"`
			ConexaoID int64  `json:"conexao_id"`
		}
		ler(&d)
		return map[string]any{"tipo": "conexao.mudou", "acao": d.Acao, "conexao_id": d.ConexaoID}
	case "banco.consulta", "banco.alteracao":
		// Sem SQL nem resultado: a tela relê o histórico se estiver nele.
		var d struct {
			ConexaoID int64 `json:"conexao_id"`
		}
		ler(&d)
		return map[string]any{"tipo": "banco.consulta", "conexao_id": d.ConexaoID, "tarefa_id": e.Escopo.Tarefa, "agente_id": e.Escopo.Agente}
	case "navegador.aberto", "navegador.fechado", "navegador.captura", "navegador.recusado":
		var d struct {
			Descricao string `json:"descricao"`
		}
		ler(&d)
		return map[string]any{"tipo": e.Tipo, "tarefa_id": e.Escopo.Tarefa, "agente_id": e.Escopo.Agente, "descricao": d.Descricao}
	}
	return nil
}

func cmpOr(a, b string) string {
	if a != "" {
		return a
	}
	return b
}

// eventos é o WebSocket que leva à tela, na hora, tudo que muda no perfil.
func (s *Servidor) eventos(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	assinatura, err := s.Avisos.Assinar(id)
	if errors.Is(err, avisos.ErrMuitasAssinaturas) {
		http.Error(w, err.Error(), http.StatusServiceUnavailable)
		return
	}
	defer assinatura.Cancelar()
	// Sem InsecureSkipVerify: uma conexão com Origin de outro site é recusada.
	conn, err := websocket.Accept(w, r, nil)
	if err != nil {
		return
	}
	defer conn.CloseNow()
	conn.SetReadLimit(limiteMensagemEventos)
	// A tela não manda comandos por aqui: o que chegar é descartado, e o
	// contexto acaba quando ela fecha a conexão.
	ctx, cancelar := context.WithCancel(r.Context())
	defer cancelar()
	go func() {
		defer cancelar()
		for {
			if _, _, err := conn.Read(ctx); err != nil {
				return
			}
		}
	}()

	ola := map[string]any{"tipo": "ola", "seq": s.Avisos.Seq(), "nucleo": s.Versao}
	if s.AvisoHistorico != "" {
		ola["aviso"] = s.AvisoHistorico
	}
	bruto, _ := json.Marshal(ola)
	if conn.Write(ctx, websocket.MessageText, bruto) != nil {
		return
	}
	for {
		m, err := assinatura.Proxima(ctx)
		if err != nil {
			return
		}
		if conn.Write(ctx, websocket.MessageText, m) != nil {
			return
		}
	}
}

// quadro é o retrato do perfil numa resposta só: projetos, tarefas e agentes,
// com o número da última mensagem de eventos já incluída nele.
func (s *Servidor) quadro(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	// Lido antes das consultas: o que mudar depois chega pelo WebSocket com seq maior.
	seq := s.Avisos.Seq()
	projetos, err := s.Banco.ListarProjetos(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	tarefas, err := s.Banco.ListarTarefasDoPerfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	agentes, err := s.Banco.ListarAgentesDoPerfil(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	fins, err := s.Banco.UltimosFins(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista := make([]agenteComEstado, 0, len(agentes))
	for _, a := range agentes {
		lista = append(lista, s.comEstado(a, "", fins))
	}
	// Os pedidos abertos e os das últimas 12 horas (o "Respondido · 21:44"
	// continua no cartão depois de reabrir a tela) e as tarefas com o
	// navegador aberto.
	pedidos, err := s.Banco.PedidosRecentesDoPerfil(r.Context(), id, time.Now().Add(-12*time.Hour).UTC().Format(time.RFC3339Nano))
	if err != nil {
		responderErro(w, err)
		return
	}
	navegadores := []int64{}
	for _, t := range tarefas {
		if s.Navegadores.Aberto(id, t.ID).Aberto {
			navegadores = append(navegadores, t.ID)
		}
	}
	responderJSON(w, map[string]any{"seq": seq, "projetos": projetos, "tarefas": tarefas, "agentes": lista, "pedidos": pedidos, "navegadores": navegadores,
		"aprovacoes": s.aprovacoesPendentes(id)})
}

// comEstado junta ao agente o que o núcleo sabe dele agora: se roda, em que
// estado está e, se parou, como terminou.
func (s *Servidor) comEstado(a dados.Agente, pasta string, fins map[int64]dados.Evento) agenteComEstado {
	r := agenteComEstado{Agente: a, Pasta: pasta}
	if sessao, ok := s.Agentes.Pegar(a.ID); ok && !sessao.Encerrada() {
		r.Ativo = true
		if estado, motivo, desde := sessao.Estado(); estado != "" {
			r.Estado, r.Motivo, r.Desde, r.DesdeHora = estado, motivo, desde.UTC().Format(time.RFC3339Nano), hora(desde)
		}
		return r
	}
	if e, ok := fins[a.ID]; ok {
		fim := fimDoEvento(e)
		r.UltimoFim = &fim
	}
	return r
}
