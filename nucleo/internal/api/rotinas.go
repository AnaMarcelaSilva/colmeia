package api

// As rotinas do workspace: pedidos guardados que o Claude Code responde em
// texto a partir do quadro de um período e dos commits dos projetos. Rodam
// quando você manda (POST .../rodar) ou no horário da agenda, que um laço
// confere a cada meio minuto enquanto o núcleo está no ar. Cada rodada vira
// um evento ao começar e ao terminar; a tela acompanha por eles.

import (
	"context"
	"errors"
	"fmt"
	"log"
	"net/http"
	"os/exec"
	"strconv"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/rotina"
)

// IntervaloAgenda é de quanto em quanto tempo o agendador olha as rotinas
// (os testes encurtam).
var IntervaloAgenda = 30 * time.Second

// Quantos commits por projeto entram no pedido.
const maxCommitsRotina = 150

type rotinasEmCurso struct {
	mu       sync.Mutex
	rodando  map[int64]bool
	parar    chan struct{}
	parou    chan struct{}
	trabalho sync.WaitGroup
}

func (s *Servidor) rotasRotinas(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/workspaces/{id}/rotinas", s.listarRotinas)
	mux.HandleFunc("POST /v1/workspaces/{id}/rotinas", s.criarRotina)
	mux.HandleFunc("PATCH /v1/rotinas/{id}", s.editarRotina)
	mux.HandleFunc("DELETE /v1/rotinas/{id}", s.removerRotina)
	mux.HandleFunc("POST /v1/rotinas/{id}/rodar", s.rodarRotina)
	mux.HandleFunc("GET /v1/rotinas/{id}/rodadas", s.listarRodadas)
	mux.HandleFunc("GET /v1/rodadas/{id}", s.lerRodada)
}

// iniciarRotinas fecha rodadas presas de um núcleo que caiu e liga o agendador.
func (s *Servidor) iniciarRotinas() {
	s.rotinas.rodando = map[int64]bool{}
	s.rotinas.parar = make(chan struct{})
	s.rotinas.parou = make(chan struct{})
	if err := s.Banco.RodadasPresas(context.Background()); err != nil {
		log.Printf("rotinas: fechando rodadas presas: %v", err)
	}
	go s.agendarRotinas()
}

// encerrarRotinas para o agendador e espera as rodadas em curso.
func (s *Servidor) encerrarRotinas() {
	if s.rotinas.parar == nil {
		return
	}
	close(s.rotinas.parar)
	<-s.rotinas.parou
	s.rotinas.trabalho.Wait()
}

type rotinaComProxima struct {
	dados.Rotina
	Proxima      string `json:"proxima,omitempty"`
	ProximaTexto string `json:"proxima_texto,omitempty"`
}

func (s *Servidor) comProximas(rotinas []dados.Rotina) []rotinaComProxima {
	agora := time.Now()
	saida := make([]rotinaComProxima, 0, len(rotinas))
	for _, r := range rotinas {
		c := rotinaComProxima{Rotina: r}
		if p, ok := proximaRodada(r, agora); ok {
			c.Proxima = p.Format(time.RFC3339)
			c.ProximaTexto = textoDoMomento(p, agora)
		}
		if c.Ultima != nil {
			u := *c.Ultima
			comTexto(&u, agora)
			c.Ultima = &u
		}
		saida = append(saida, c)
	}
	return saida
}

// comTexto preenche o momento da rodada como a tela mostra.
func comTexto(r *dados.Rodada, agora time.Time) {
	if t, err := time.Parse(time.RFC3339Nano, r.Momento); err == nil {
		r.MomentoTexto = textoDoMomento(t, agora)
	}
}

func comTextos(rodadas []dados.Rodada) []dados.Rodada {
	agora := time.Now()
	for i := range rodadas {
		comTexto(&rodadas[i], agora)
	}
	return rodadas
}

// proximaRodada é o próximo instante da agenda a partir de `agora`, no fuso
// do computador. Sem agenda (ou inativa), nada.
func proximaRodada(r dados.Rotina, agora time.Time) (time.Time, bool) {
	if !r.Ativa || len(r.AgendaDias) == 0 {
		return time.Time{}, false
	}
	hora, err := time.Parse("15:04", r.AgendaHora)
	if err != nil {
		return time.Time{}, false
	}
	dias := map[int]bool{}
	for _, d := range r.AgendaDias {
		dias[d] = true
	}
	agora = agora.In(time.Local)
	for i := 0; i < 8; i++ {
		dia := agora.AddDate(0, 0, i)
		// time.Weekday: domingo = 0; na agenda, segunda = 1 … domingo = 7.
		semana := int(dia.Weekday())
		if semana == 0 {
			semana = 7
		}
		if !dias[semana] {
			continue
		}
		candidato := time.Date(dia.Year(), dia.Month(), dia.Day(), hora.Hour(), hora.Minute(), 0, 0, time.Local)
		if candidato.After(agora) {
			return candidato, true
		}
	}
	return time.Time{}, false
}

var diasDaSemana = []string{"domingo", "segunda", "terça", "quarta", "quinta", "sexta", "sábado"}

// textoDoMomento: "hoje às 17:00", "amanhã às 08:30", "sexta, 24/10 às 17:00".
func textoDoMomento(t, agora time.Time) string {
	t, agora = t.In(time.Local), agora.In(time.Local)
	hoje := time.Date(agora.Year(), agora.Month(), agora.Day(), 0, 0, 0, 0, time.Local)
	dia := time.Date(t.Year(), t.Month(), t.Day(), 0, 0, 0, 0, time.Local)
	hora := t.Format("15:04")
	switch int(dia.Sub(hoje).Hours() / 24) {
	case 0:
		return "hoje às " + hora
	case 1:
		return "amanhã às " + hora
	case -1:
		return "ontem às " + hora
	}
	return fmt.Sprintf("%s, %s às %s", diasDaSemana[t.Weekday()], t.Format("02/01"), hora)
}

func (s *Servidor) listarRotinas(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Workspace(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	rotinas, err := s.Banco.ListarRotinas(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"rotinas": s.comProximas(rotinas), "claude": s.temClaude()})
}

// temClaude diz se o Claude Code está instalado (as rotinas precisam dele).
func (s *Servidor) temClaude() bool {
	_, err := exec.LookPath(claudecode.Executavel)
	return err == nil
}

func (s *Servidor) criarRotina(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var nova dados.NovaRotina
	if err := ler(r, &nova); err != nil {
		responderErro(w, err)
		return
	}
	rotina, err := s.Banco.CriarRotina(r.Context(), id, nova)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.comProximas([]dados.Rotina{rotina})[0])
}

func (s *Servidor) editarRotina(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var nova dados.NovaRotina
	if err := ler(r, &nova); err != nil {
		responderErro(w, err)
		return
	}
	rotina, err := s.Banco.EditarRotina(r.Context(), id, nova)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.comProximas([]dados.Rotina{rotina})[0])
}

func (s *Servidor) removerRotina(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Banco.RemoverRotina(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) listarRodadas(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	max, _ := strconv.Atoi(r.URL.Query().Get("max"))
	rodadas, err := s.Banco.ListarRodadas(r.Context(), id, max)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"rodadas": comTextos(rodadas)})
}

func (s *Servidor) lerRodada(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	rodada, err := s.Banco.Rodada(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	comTexto(&rodada, time.Now())
	responderJSON(w, rodada)
}

// rodarRotina: POST /v1/rotinas/{id}/rodar começa uma rodada e responde na
// hora com ela em "rodando"; o resultado chega pelo evento rodada.terminou.
func (s *Servidor) rodarRotina(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	rodada, err := s.comecarRodada(r.Context(), id, dados.OrigemVoce)
	if err != nil {
		responderErro(w, err)
		return
	}
	comTexto(&rodada, time.Now())
	responderJSON(w, rodada)
}

// comecarRodada abre a rodada e dispara o trabalho numa goroutine. Uma
// rotina roda uma vez de cada vez.
func (s *Servidor) comecarRodada(ctx context.Context, id int64, origem string) (dados.Rodada, error) {
	if !s.temClaude() {
		return dados.Rodada{}, dados.ErrInvalido{Motivo: claudecode.SemClaude}
	}
	rot, err := s.Banco.Rotina(ctx, id)
	if err != nil {
		return dados.Rodada{}, err
	}
	s.rotinas.mu.Lock()
	if s.rotinas.rodando[id] {
		s.rotinas.mu.Unlock()
		return dados.Rodada{}, dados.ErrInvalido{Motivo: "esta rotina já está rodando; espere ela terminar"}
	}
	s.rotinas.rodando[id] = true
	s.rotinas.mu.Unlock()
	de, ate := s.periodoDaRotina(ctx, rot, time.Now())
	rodada, err := s.Banco.ComecarRodada(ctx, id, origem, de.Format("2006-01-02"), ate.Format("2006-01-02"))
	if err != nil {
		s.rotinas.mu.Lock()
		delete(s.rotinas.rodando, id)
		s.rotinas.mu.Unlock()
		return dados.Rodada{}, err
	}
	s.rotinas.trabalho.Add(1)
	go func() {
		defer s.rotinas.trabalho.Done()
		defer func() {
			s.rotinas.mu.Lock()
			delete(s.rotinas.rodando, id)
			s.rotinas.mu.Unlock()
		}()
		ctx := context.Background()
		texto, err := s.produzirRotina(ctx, rot, de, ate)
		erro := ""
		if err != nil {
			erro = mensagemDeErro(err)
		}
		if _, err := s.Banco.TerminarRodada(ctx, rodada.ID, texto, rotina.Resumo(texto), erro); err != nil {
			log.Printf("rotina %d: fechando a rodada %d: %v", id, rodada.ID, err)
		}
	}()
	return rodada, nil
}

// periodoDaRotina: os últimos `Dias` dias até hoje, desde a última rodada
// que deu certo (com Dias 0; sem rodada, 7 dias) ou desde a tag mais antiga
// entre as últimas tags dos projetos git do workspace (sem tag, 30 dias).
func (s *Servidor) periodoDaRotina(ctx context.Context, r dados.Rotina, agora time.Time) (de, ate time.Time) {
	agora = agora.In(time.Local)
	ate = time.Date(agora.Year(), agora.Month(), agora.Day(), 0, 0, 0, 0, time.Local)
	dias := r.Dias
	if dias == dados.DiasDesdeATag {
		dias = 30
		if perfil, err := s.Banco.PerfilDaRotina(ctx, r.ID); err == nil {
			for _, p := range s.projetosComGit(ctx, perfil, r.WorkspaceID) {
				if _, data, err := git.UltimaTag(ctx, p.Caminho); err == nil && data != "" {
					if t, err := time.ParseInLocation("2006-01-02", data, time.Local); err == nil && (de.IsZero() || t.Before(de)) {
						de = t
					}
				}
			}
		}
		if !de.IsZero() {
			return de, ate
		}
	}
	if dias == 0 {
		dias = 7
		if ultima, err := s.Banco.UltimaRodada(ctx, r.ID, ""); err == nil && ultima.Estado == dados.RodadaOk {
			if t, err := time.ParseInLocation("2006-01-02", ultima.Ate, time.Local); err == nil {
				return t, ate
			}
		}
	}
	return ate.AddDate(0, 0, -dias), ate
}

// produzirRotina junta o quadro e os commits do período e pede o texto.
func (s *Servidor) produzirRotina(ctx context.Context, r dados.Rotina, de, ate time.Time) (string, error) {
	executavel, err := exec.LookPath(claudecode.Executavel)
	if err != nil {
		return "", dados.ErrInvalido{Motivo: claudecode.SemClaude}
	}
	perfilID, err := s.Banco.PerfilDaRotina(ctx, r.ID)
	if err != nil {
		return "", err
	}
	perfil, err := s.Banco.Perfil(ctx, perfilID)
	if err != nil {
		return "", err
	}
	workspace, err := s.Banco.Workspace(ctx, r.WorkspaceID)
	if err != nil {
		return "", err
	}
	entrada := rotina.Entrada{Workspace: workspace.Nome, De: de.Format("02/01/2006"), Ate: ate.Format("02/01/2006"),
		Periodo: fmt.Sprintf("%s a %s", de.Format("02/01"), ate.Format("02/01"))}
	if r.Dias == dados.DiasDesdeATag {
		entrada.Periodo = fmt.Sprintf("desde a última tag, %s a %s", de.Format("02/01"), ate.Format("02/01"))
	}
	if r.DoQuadro {
		entrada.Quadro, err = s.quadroDoPeriodo(ctx, perfil, r.WorkspaceID, de, ate)
		if err != nil {
			return "", err
		}
	}
	if r.DoCodigo {
		entrada.Commits = s.commitsDoPeriodo(ctx, perfil.ID, r.WorkspaceID, de, ate, r.Dias == dados.DiasDesdeATag)
	}
	env, err := s.contaDoClaude(ctx, perfil.ID)
	if err != nil {
		return "", err
	}
	return rotina.Gerador{Executavel: executavel, Env: env}.Gerar(ctx, r.Nome, r.Pedido, entrada)
}

// quadroDoPeriodo é o resumo da sprint do workspace no período, em Markdown:
// o mesmo texto que a tela da sprint mostra e copia.
func (s *Servidor) quadroDoPeriodo(ctx context.Context, perfil dados.Perfil, workspace int64, de, ate time.Time) (string, error) {
	rc := recorte{Workspace: workspace}
	c, err := s.contextoDaLinha(ctx, perfil, rc)
	if errors.Is(err, dados.ErrNaoEncontrado) {
		// Workspace sem projetos: não há quadro.
		return "", nil
	}
	if err != nil {
		return "", err
	}
	eventos, err := s.Banco.ListarEventos(ctx, rc.ate(perfil.ID, inicioDoDia(ate.AddDate(0, 0, 1))))
	if err != nil {
		return "", err
	}
	resumo := linha.Sprint(eventos, de, ate, c)
	if resumo.Vazio {
		return "", nil
	}
	return resumo.Markdown, nil
}

// projetosComGit: os projetos do workspace que têm repositório, do tipo git
// ou uma pasta que por acaso é um repositório.
func (s *Servidor) projetosComGit(ctx context.Context, perfil, workspace int64) []dados.Projeto {
	projetos, err := s.Banco.ListarProjetos(ctx, perfil)
	if err != nil {
		return nil
	}
	var saida []dados.Projeto
	for _, p := range projetos {
		if p.WorkspaceID != workspace {
			continue
		}
		if p.Tipo != "git" {
			if _, _, err := git.Repositorio(ctx, p.Caminho); err != nil {
				continue
			}
		}
		saida = append(saida, p)
	}
	return saida
}

// commitsDoPeriodo lê os commits de cada projeto com git do workspace: no
// período, ou desde a última tag de cada um (sem tag, o período).
func (s *Servidor) commitsDoPeriodo(ctx context.Context, perfil, workspace int64, de, ate time.Time, desdeATag bool) []rotina.ProjetoCommits {
	var saida []rotina.ProjetoCommits
	for _, p := range s.projetosComGit(ctx, perfil, workspace) {
		pc := rotina.ProjetoCommits{Projeto: p.Nome}
		var err error
		if desdeATag {
			if tag, data, e := git.UltimaTag(ctx, p.Caminho); e == nil && tag != "" {
				pc.Desde = "desde a tag " + tag
				if t, e := time.Parse("2006-01-02", data); e == nil {
					pc.Desde += " (" + t.Format("02/01/2006") + ")"
				}
				pc.Commits, pc.Mais, err = git.CommitsDesde(ctx, p.Caminho, tag, maxCommitsRotina)
			} else {
				pc.Desde = "sem tag; commits do período"
			}
		}
		if pc.Desde == "" || pc.Desde == "sem tag; commits do período" {
			pc.Commits, pc.Mais, err = git.Commits(ctx, p.Caminho, de.Format(time.RFC3339), ate.AddDate(0, 0, 1).Format(time.RFC3339), maxCommitsRotina)
		}
		if err != nil {
			pc.Erro = mensagemDeErro(err)
		}
		saida = append(saida, pc)
	}
	return saida
}

// agendarRotinas confere a cada IntervaloAgenda se alguma rotina agendada
// venceu: o dia da semana está na agenda, já passou da hora, e a última
// rodada da agenda é anterior a esse horário.
func (s *Servidor) agendarRotinas() {
	defer close(s.rotinas.parou)
	tique := time.NewTicker(IntervaloAgenda)
	defer tique.Stop()
	for {
		select {
		case <-s.rotinas.parar:
			return
		case agora := <-tique.C:
			s.dispararVencidas(agora)
		}
	}
}

func (s *Servidor) dispararVencidas(agora time.Time) {
	ctx := context.Background()
	rotinas, err := s.Banco.RotinasAgendadas(ctx)
	if err != nil {
		log.Printf("rotinas: lendo a agenda: %v", err)
		return
	}
	for _, r := range rotinas {
		vencida, ok := horarioVencido(r, agora)
		if !ok {
			continue
		}
		// Um horário anterior à última mudança da rotina não conta: criar ou
		// editar uma rotina de tarde com a agenda às 8h não a roda na hora.
		if t, err := time.Parse(time.RFC3339Nano, r.AtualizadoEm); err == nil && vencida.Before(t) {
			continue
		}
		if ultima, err := s.Banco.UltimaRodada(ctx, r.ID, dados.OrigemAgenda); err == nil {
			if t, err := time.Parse(time.RFC3339Nano, ultima.Momento); err == nil && !t.Before(vencida) {
				continue
			}
		}
		if _, err := s.comecarRodada(ctx, r.ID, dados.OrigemAgenda); err != nil {
			log.Printf("rotina %d (%s): pela agenda: %v", r.ID, r.Nome, err)
		}
	}
}

// horarioVencido é o último horário da agenda que já passou hoje (só hoje:
// um horário perdido com a Colmeia fechada não roda no dia seguinte).
func horarioVencido(r dados.Rotina, agora time.Time) (time.Time, bool) {
	hora, err := time.Parse("15:04", r.AgendaHora)
	if err != nil {
		return time.Time{}, false
	}
	agora = agora.In(time.Local)
	semana := int(agora.Weekday())
	if semana == 0 {
		semana = 7
	}
	na := false
	for _, d := range r.AgendaDias {
		na = na || d == semana
	}
	if !na {
		return time.Time{}, false
	}
	hoje := time.Date(agora.Year(), agora.Month(), agora.Day(), hora.Hour(), hora.Minute(), 0, 0, time.Local)
	if agora.Before(hoje) {
		return time.Time{}, false
	}
	return hoje, true
}
