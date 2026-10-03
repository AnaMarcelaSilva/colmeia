package api

import (
	"context"
	"net/http"
	"regexp"
	"strconv"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
)

const (
	limitePadraoLinha = 200
	limiteMaximoLinha = 500
)

var padraoData = regexp.MustCompile(`^\d{4}-\d{2}-\d{2}$`)

func (s *Servidor) rotasLinha(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/perfis/{id}/linha-do-tempo", s.linhaDoTempo)
	mux.HandleFunc("GET /v1/perfis/{id}/resumo", s.resumo)
	mux.HandleFunc("GET /v1/perfis/{id}/apresentacao", s.apresentacao)
}

// data lê AAAA-MM-DD no fuso local.
func data(campo, valor string) (time.Time, error) {
	if !padraoData.MatchString(valor) {
		return time.Time{}, dados.ErrInvalido{Motivo: campo + " precisa estar no formato AAAA-MM-DD."}
	}
	t, err := time.ParseInLocation("2006-01-02", valor, time.Local)
	if err != nil {
		return time.Time{}, dados.ErrInvalido{Motivo: campo + " não é uma data válida."}
	}
	return t, nil
}

// periodo lê de/ate (opcionais) e confere a ordem e o tamanho. A tela não
// lida com fuso: "ultimos=7" (os últimos 7 dias, com hoje) e "mes=atual"
// (do dia 1 até hoje) são calculados aqui.
func periodo(r *http.Request, obrigatorio bool) (de, ate time.Time, err error) {
	q := r.URL.Query()
	agora := time.Now()
	hoje := time.Date(agora.Year(), agora.Month(), agora.Day(), 0, 0, 0, 0, time.Local)
	if v := q.Get("ultimos"); v != "" {
		dias, erro := strconv.Atoi(v)
		if erro != nil || dias < 1 || dias > linha.MaxDiasSprint {
			err = dados.ErrInvalido{Motivo: "O período pode ter de 1 a " + strconv.Itoa(linha.MaxDiasSprint) + " dias."}
			return
		}
		return hoje.AddDate(0, 0, 1-dias), hoje, nil
	}
	if v := q.Get("mes"); v != "" {
		if v != "atual" {
			err = dados.ErrInvalido{Motivo: "mes precisa ser \"atual\"."}
			return
		}
		return time.Date(hoje.Year(), hoje.Month(), 1, 0, 0, 0, 0, time.Local), hoje, nil
	}
	if q.Get("de") == "" && q.Get("ate") == "" && !obrigatorio {
		return
	}
	if de, err = data("A data inicial", q.Get("de")); err != nil {
		return
	}
	if ate, err = data("A data final", q.Get("ate")); err != nil {
		return
	}
	if ate.Before(de) {
		err = dados.ErrInvalido{Motivo: "A data final vem antes da inicial."}
		return
	}
	if ate.Sub(de) > (linha.MaxDiasSprint-1)*24*time.Hour+time.Hour {
		err = dados.ErrInvalido{Motivo: "O período pode ter no máximo " + strconv.Itoa(linha.MaxDiasSprint) + " dias."}
	}
	return
}

// contextoDaLinha junta o que a linha do tempo precisa saber do estado atual
// do perfil (ou de um projeto dele).
func (s *Servidor) contextoDaLinha(ctx context.Context, perfil, projeto int64) (linha.Contexto, error) {
	c := linha.Contexto{Agora: time.Now(), Fuso: time.Local, Projetos: map[int64]string{}, Tarefas: map[int64]linha.TarefaAtual{}, Ativos: map[int64]linha.Ativo{}}
	projetos, err := s.Banco.ListarProjetos(ctx, perfil)
	if err != nil {
		return c, err
	}
	achou := projeto == 0
	for _, p := range projetos {
		c.Projetos[p.ID] = p.Nome
		if projeto == 0 || p.ID == projeto {
			c.NomesNoEscopo = append(c.NomesNoEscopo, p.Nome)
		}
		achou = achou || p.ID == projeto
	}
	if !achou {
		return c, dados.ErrNaoEncontrado
	}
	c.VariosProjetos = projeto == 0 && len(projetos) > 1
	tarefas, err := s.Banco.ListarTarefasDoPerfil(ctx, perfil)
	if err != nil {
		return c, err
	}
	for _, t := range tarefas {
		if projeto == 0 || t.ProjetoID == projeto {
			c.Tarefas[t.ID] = linha.TarefaAtual{ID: t.ID, Titulo: t.Titulo, Coluna: t.Coluna, ProjetoID: t.ProjetoID}
		}
	}
	for _, sessao := range s.Agentes.Sessoes() {
		if !sessao.Encerrada() {
			estado, _, desde := sessao.Estado()
			ativo := linha.Ativo{Estado: estado, Desde: desde}
			if ctxAgente, ok := s.contexto(sessao.ID); ok {
				ativo.Tarefa = ctxAgente.TarefaID
			}
			c.Ativos[sessao.ID] = ativo
		}
	}
	c.AnexosRemovidos, err = s.Banco.AnexosRemovidos(ctx, perfil)
	return c, err
}

func projetoDoPedido(r *http.Request) (int64, error) {
	v := r.URL.Query().Get("projeto")
	if v == "" {
		return 0, nil
	}
	id, err := strconv.ParseInt(v, 10, 64)
	if err != nil || id <= 0 {
		return 0, dados.ErrInvalido{Motivo: "projeto inválido"}
	}
	return id, nil
}

func inicioDoDia(t time.Time) string { return t.UTC().Format(time.RFC3339Nano) }

// linhaDoTempo devolve os dias do perfil, do mais novo para o mais antigo, em
// páginas de eventos: `antes` é o `proximo` da página anterior.
func (s *Servidor) linhaDoTempo(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	p, err := s.Banco.Perfil(r.Context(), perfil)
	if err != nil {
		responderErro(w, err)
		return
	}
	projeto, err := projetoDoPedido(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	de, ate, err := periodo(r, false)
	if err != nil {
		responderErro(w, err)
		return
	}
	q := r.URL.Query()
	limite := limitePadraoLinha
	if v := q.Get("limite"); v != "" {
		if limite, err = strconv.Atoi(v); err != nil || limite < 1 || limite > limiteMaximoLinha {
			responderErro(w, dados.ErrInvalido{Motivo: "limite precisa estar entre 1 e 500"})
			return
		}
	}
	var antes int64
	if v := q.Get("antes"); v != "" {
		if antes, err = strconv.ParseInt(v, 10, 64); err != nil || antes <= 0 {
			responderErro(w, dados.ErrInvalido{Motivo: "antes inválido"})
			return
		}
	}
	c, err := s.contextoDaLinha(r.Context(), perfil, projeto)
	if err != nil {
		responderErro(w, err)
		return
	}
	filtro := dados.FiltroEventos{Perfil: perfil, Projeto: projeto, Antes: antes, Limite: limite}
	if !de.IsZero() {
		filtro.Desde, filtro.Ate = inicioDoDia(de), inicioDoDia(ate.AddDate(0, 0, 1))
	}
	eventos, err := s.Banco.ListarEventos(r.Context(), filtro)
	if err != nil {
		responderErro(w, err)
		return
	}
	var proximo int64
	if len(eventos) == limite {
		proximo = eventos[len(eventos)-1].ID
	}
	criado := p.CriadoEm
	if t, err := time.Parse(time.RFC3339Nano, p.CriadoEm); err == nil {
		criado = t.Local().Format("02/01/2006")
	}
	responderJSON(w, map[string]any{"dias": linha.Montar(eventos, c), "proximo": proximo, "perfil_criado_em": criado})
}

// resumo monta a daily (tipo=daily) ou a sprint (tipo=sprint&de=&ate=), em
// JSON ou, com formato=markdown, pronto para salvar.
func (s *Servidor) resumo(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), perfil); err != nil {
		responderErro(w, err)
		return
	}
	projeto, err := projetoDoPedido(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	q := r.URL.Query()
	formato := q.Get("formato")
	if formato != "" && formato != "json" && formato != "markdown" {
		responderErro(w, dados.ErrInvalido{Motivo: "formato precisa ser json ou markdown"})
		return
	}
	c, err := s.contextoDaLinha(r.Context(), perfil, projeto)
	if err != nil {
		responderErro(w, err)
		return
	}
	switch q.Get("tipo") {
	case "daily":
		hoje := time.Now()
		inicio := time.Date(hoje.Year(), hoje.Month(), hoje.Day(), 0, 0, 0, 0, time.Local).AddDate(0, 0, -linha.JanelaDaily)
		eventos, err := s.Banco.ListarEventos(r.Context(), dados.FiltroEventos{Perfil: perfil, Projeto: projeto, Desde: inicioDoDia(inicio)})
		if err != nil {
			responderErro(w, err)
			return
		}
		d := linha.Daily(eventos, c)
		if formato == "markdown" {
			responderMarkdown(w, d.Texto+"\n")
			return
		}
		responderJSON(w, d)
	case "sprint":
		de, ate, err := periodo(r, true)
		if err != nil {
			responderErro(w, err)
			return
		}
		// O histórico até o fim do período: o anterior diz onde cada tarefa estava.
		eventos, err := s.Banco.ListarEventos(r.Context(), dados.FiltroEventos{Perfil: perfil, Projeto: projeto, Ate: inicioDoDia(ate.AddDate(0, 0, 1))})
		if err != nil {
			responderErro(w, err)
			return
		}
		sprint := linha.Sprint(eventos, de, ate, c)
		if formato == "markdown" {
			responderMarkdown(w, sprint.Markdown)
			return
		}
		responderJSON(w, sprint)
	default:
		responderErro(w, dados.ErrInvalido{Motivo: "tipo precisa ser daily ou sprint"})
	}
}

func responderMarkdown(w http.ResponseWriter, texto string) {
	w.Header().Set("Content-Type", "text/markdown; charset=utf-8")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Write([]byte(texto))
}

// apresentacao monta o deck da daily (tipo=daily) ou da sprint
// (tipo=sprint, com o período como no resumo): uma tarefa por slide, com os
// anexos e as notas de cada uma.
func (s *Servidor) apresentacao(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), perfil); err != nil {
		responderErro(w, err)
		return
	}
	projeto, err := projetoDoPedido(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	tipo := r.URL.Query().Get("tipo")
	if tipo != "daily" && tipo != "sprint" {
		responderErro(w, dados.ErrInvalido{Motivo: "tipo precisa ser daily ou sprint"})
		return
	}
	c, err := s.contextoDaLinha(r.Context(), perfil, projeto)
	if err != nil {
		responderErro(w, err)
		return
	}
	var deck linha.Deck
	if tipo == "daily" {
		hoje := time.Now()
		inicio := time.Date(hoje.Year(), hoje.Month(), hoje.Day(), 0, 0, 0, 0, time.Local).AddDate(0, 0, -linha.JanelaDaily)
		eventos, err := s.Banco.ListarEventos(r.Context(), dados.FiltroEventos{Perfil: perfil, Projeto: projeto, Desde: inicioDoDia(inicio)})
		if err != nil {
			responderErro(w, err)
			return
		}
		deck = linha.Apresentacao(eventos, time.Time{}, time.Time{}, c, tipo)
	} else {
		de, ate, err := periodo(r, true)
		if err != nil {
			responderErro(w, err)
			return
		}
		eventos, err := s.Banco.ListarEventos(r.Context(), dados.FiltroEventos{Perfil: perfil, Projeto: projeto, Ate: inicioDoDia(ate.AddDate(0, 0, 1))})
		if err != nil {
			responderErro(w, err)
			return
		}
		deck = linha.Apresentacao(eventos, de, ate, c, tipo)
	}
	ids := deck.Tarefas()
	inicio, _ := time.ParseInLocation("2006-01-02", deck.De, time.Local)
	fim, _ := time.ParseInLocation("2006-01-02", deck.Ate, time.Local)
	anexos, err := s.Banco.AnexosDasTarefas(r.Context(), ids, inicioDoDia(inicio), inicioDoDia(fim.AddDate(0, 0, 1)))
	if err != nil {
		responderErro(w, err)
		return
	}
	notas, err := s.Banco.NotasDoPeriodo(r.Context(), ids, tipo, deck.ChaveNota)
	if err != nil {
		responderErro(w, err)
		return
	}
	var anteriores map[int64]dados.Nota
	if tipo == "sprint" {
		if anteriores, err = s.Banco.UltimasNotas(r.Context(), ids, tipo); err != nil {
			responderErro(w, err)
			return
		}
	}
	deck.Completar(anexos, notas, anteriores)
	responderJSON(w, deck)
}
