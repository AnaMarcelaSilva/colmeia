package api

import (
	"context"
	"encoding/json"
	"errors"
	"log"
	"net/http"
	"os/exec"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/claudecode"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/linha"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/montagem"
)

// A montagem da reunião (pacote montagem): a cada mudança de uma tarefa
// (nota, anexo, coluna, o agente terminou a vez), o núcleo refaz a montagem
// simples dela na hora e, com o montador ligado no perfil, pede ao Claude
// Code a montagem com IA depois de um tempo quieto, uma por vez. As lousas
// da daily de hoje e da sprint atual, quando existem, são redesenhadas com
// as montagens novas. A tela fica sabendo por "montagem.mudou" e
// "lousa.mudou".

var (
	// EsperaSimples junta mudanças seguidas antes da montagem simples.
	EsperaSimples = 1500 * time.Millisecond
	// EsperaMontador é o tempo quieto antes de pedir a montagem com IA (os
	// testes encurtam).
	EsperaMontador = 3 * time.Minute
	// AnexosDaMontagem: só as imagens destes últimos dias entram.
	AnexosDaMontagem = 14
)

type montagens struct {
	mu sync.Mutex
	// montando serializa as montagens feitas por falta dentro de um pedido
	// (a página e a lousa da reunião): pedidos ao mesmo tempo não montam a
	// mesma tarefa duas vezes.
	montando   sync.Mutex
	simples    map[int64]*time.Timer
	montador   map[int64]*time.Timer
	perfil     map[int64]int64
	gerando    bool
	encerrando bool
	trabalhos  sync.WaitGroup
}

func (m *montagens) iniciar() {
	m.simples = map[int64]*time.Timer{}
	m.montador = map[int64]*time.Timer{}
	m.perfil = map[int64]int64{}
}

func (m *montagens) encerrar() {
	m.mu.Lock()
	m.encerrando = true
	for _, t := range m.simples {
		t.Stop()
	}
	for _, t := range m.montador {
		t.Stop()
	}
	m.mu.Unlock()
	m.trabalhos.Wait()
}

// rodar faz o trabalho fora de quem gravou o evento, nunca depois do encerramento.
func (m *montagens) rodar(trabalho func()) {
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.encerrando {
		return
	}
	m.trabalhos.Add(1)
	go func() {
		defer m.trabalhos.Done()
		trabalho()
	}()
}

// aoGravarParaMontagem olha os eventos gravados: o que muda o que a reunião
// mostra de uma tarefa agenda a montagem dela.
func (s *Servidor) aoGravarParaMontagem(e dados.Evento) {
	switch e.Tipo {
	case "nota.atualizada", "anexo.adicionado", "anexo.removido", "tarefa.atualizada", "tarefa.transferida",
		"lousa.agente", "agente.terminou", "pedido.respondido":
		if e.Escopo.Tarefa != 0 {
			s.agendarMontagem(e.Escopo.Perfil, e.Escopo.Tarefa, false)
		}
	case "tarefa.removida", "daily.fora", "sprint.fora":
		// A lousa da reunião perde ou ganha uma coluna.
		s.montagens.rodar(func() { s.remontarLousasDaReuniao(context.Background(), e.Escopo.Perfil) })
	}
}

// agendarMontagem marca a tarefa: a simples sai logo, a com IA depois do
// tempo quieto (ou já, com `agora`).
func (s *Servidor) agendarMontagem(perfil, tarefa int64, agora bool) {
	m := &s.montagens
	m.mu.Lock()
	defer m.mu.Unlock()
	if m.encerrando {
		return
	}
	m.perfil[tarefa] = perfil
	if t, ok := m.simples[tarefa]; ok {
		t.Stop()
	}
	m.simples[tarefa] = time.AfterFunc(EsperaSimples, func() { m.rodar(func() { s.montarSimples(tarefa, true, true) }) })
	espera := EsperaMontador
	if agora {
		espera = 0
	}
	if t, ok := m.montador[tarefa]; ok {
		t.Stop()
	}
	m.montador[tarefa] = time.AfterFunc(espera, func() { m.rodar(func() { s.montarComIA(tarefa) }) })
}

// estadoLegivel: a coluna da tarefa como a reunião fala dela.
func estadoLegivel(coluna string, erro bool) string {
	switch {
	case erro:
		return "Parou com erro"
	case coluna == "concluido":
		return "Concluída"
	case coluna == "revisao":
		return "Em revisão"
	case coluna == "aguardando":
		return "Esperando você"
	case coluna == "trabalhando":
		return "Em andamento"
	}
	return "Na fila"
}

// entradaDaTarefa junta o que se sabe da tarefa para a montagem: o que
// aconteceu (como no slide), a nota da daily de hoje (ou a descrição), as
// imagens dos últimos dias e os textos da lousa dela.
func (s *Servidor) entradaDaTarefa(ctx context.Context, tarefa int64) (montagem.Entrada, int64, error) {
	t, projeto, perfil, err := s.Banco.Tarefa(ctx, tarefa)
	if err != nil {
		return montagem.Entrada{}, 0, err
	}
	p, err := s.Banco.Perfil(ctx, perfil)
	if err != nil {
		return montagem.Entrada{}, 0, err
	}
	e := montagem.Entrada{TarefaID: t.ID, Titulo: t.Titulo, Projeto: projeto.Nome, Workspace: projeto.Workspace, Estado: estadoLegivel(t.Coluna, false)}
	c, err := s.contextoDaLinha(ctx, p, recorte{Projeto: projeto.ID})
	if err != nil {
		return e, perfil, err
	}
	inicio := time.Now().AddDate(0, 0, -linha.JanelaDaily)
	eventos, err := s.Banco.ListarEventos(ctx, dados.FiltroEventos{Perfil: perfil, Tarefa: t.ID, Desde: inicioDoDia(inicio)})
	if err != nil {
		return e, perfil, err
	}
	deck := linha.Apresentacao(eventos, time.Time{}, time.Time{}, c, "daily")
	for _, sl := range deck.Slides {
		if sl.TarefaID != t.ID {
			continue
		}
		if sl.Numeros.Erros > 0 && sl.Coluna != "concluido" && sl.Coluna != "revisao" {
			e.Estado = estadoLegivel(t.Coluna, true)
		}
		for _, f := range sl.Feito {
			e.Feito = append(e.Feito, f.Itens...)
		}
	}
	nota, err := s.Banco.Nota(ctx, t.ID, "daily", hoje())
	if err != nil {
		return e, perfil, err
	}
	e.Nota = strings.TrimSpace(nota.Texto)
	if e.Nota == "" {
		if d, err := s.Banco.DescricaoDaTarefa(ctx, t.ID); err == nil {
			e.Descricao = strings.TrimSpace(d.Texto)
		}
	}
	anexos, err := s.Banco.AnexosDasTarefas(ctx, []int64{t.ID}, inicioDoDia(time.Now().AddDate(0, 0, -AnexosDaMontagem)), "")
	if err != nil {
		return e, perfil, err
	}
	// As mais novas primeiro.
	for i := len(anexos) - 1; i >= 0; i-- {
		a := anexos[i]
		if a.Tipo != "imagem" {
			continue
		}
		autor := "voce"
		if a.Origem == "agente" || a.Origem == "navegador" {
			autor = "agente"
		}
		e.Imagens = append(e.Imagens, montagem.Imagem{ID: a.ID, Nome: a.Nome, Legenda: a.Legenda, Quando: dataCurta(a.CriadoEm[:min(10, len(a.CriadoEm))]), Autor: autor, Largura: a.Largura, Altura: a.Altura})
	}
	if l, ok, err := s.Banco.LousaExistente(ctx, dados.DonoLousa{TarefaID: t.ID}); err == nil && ok {
		if _, elementos, err := s.Banco.Lousa(ctx, l.ID); err == nil {
			for _, el := range elementos {
				if el.Tipo == "ligacao" || (el.Titulo == "" && el.Texto == "") || len(e.Lousa) == 10 {
					continue
				}
				e.Lousa = append(e.Lousa, claudecode.UmaLinha(strings.TrimSpace(el.Titulo+" "+el.Texto), 160))
			}
		}
	}
	return e, perfil, nil
}

// guardada converte a montagem para o banco (os cartões como JSON).
func guardada(m montagem.Montagem) (dados.Montagem, error) {
	cartoes, err := json.Marshal(m.Cartoes)
	if err != nil {
		return dados.Montagem{}, err
	}
	return dados.Montagem{TarefaID: m.TarefaID, Topico: m.Topico, Frase: m.Frase, Cartoes: cartoes, Origem: m.Origem, Base: m.Base, MontadaEm: m.MontadaEm}, nil
}

// montada converte a montagem do banco para o pacote.
func montada(g dados.Montagem) montagem.Montagem {
	m := montagem.Montagem{TarefaID: g.TarefaID, Topico: g.Topico, Frase: g.Frase, Origem: g.Origem, Base: g.Base, MontadaEm: g.MontadaEm}
	json.Unmarshal(g.Cartoes, &m.Cartoes)
	return m
}

// montarSimples refaz a montagem simples da tarefa e guarda, a não ser que a
// guardada seja do montador (ela é melhor; veja abaixo). Com `redesenhar`,
// as lousas da reunião do perfil são refeitas.
// montarSimples monta e guarda a simples. Com `avisar`, a tela recebe
// montagem.mudou; a montagem feita por falta dentro de um pedido vai na
// própria resposta e não avisa (cada aviso faria a tela pedir de novo).
func (s *Servidor) montarSimples(tarefa int64, redesenhar, avisar bool) (montagem.Montagem, error) {
	ctx := context.Background()
	e, perfil, err := s.entradaDaTarefa(ctx, tarefa)
	if err != nil {
		if !errors.Is(err, dados.ErrNaoEncontrado) {
			log.Printf("tarefa %d: montando para a reunião: %v", tarefa, err)
		}
		return montagem.Montagem{}, err
	}
	m := montagem.Simples(e)
	// Uma montagem do montador fica até ele montar de novo (ela é melhor e
	// o montador já está agendado): a simples só a troca com o montador
	// desligado no perfil.
	if atual, ok, err := s.Banco.MontagemDaTarefa(ctx, tarefa); err == nil && ok && atual.Origem == montagem.OrigemMontador {
		if p, err := s.Banco.Perfil(ctx, perfil); err == nil && (p.Montador || atual.Base == m.Base) {
			return montada(atual), nil
		}
	}
	return s.guardarMontagem(ctx, perfil, m, redesenhar, avisar)
}

func (s *Servidor) guardarMontagem(ctx context.Context, perfil int64, m montagem.Montagem, redesenhar, avisar bool) (montagem.Montagem, error) {
	g, err := guardada(m)
	if err != nil {
		return m, err
	}
	if g, err = s.Banco.GuardarMontagem(ctx, g); err != nil {
		if !errors.Is(err, dados.ErrNaoEncontrado) {
			log.Printf("tarefa %d: guardando a montagem: %v", m.TarefaID, err)
		}
		return m, err
	}
	m.MontadaEm = g.MontadaEm
	if avisar {
		s.Avisos.Publicar(perfil, map[string]any{"tipo": "montagem.mudou", "tarefa_id": m.TarefaID, "montagem": m})
	}
	if redesenhar {
		s.remontarLousasDaReuniao(ctx, perfil)
	}
	return m, nil
}

// montarComIA pede a montagem ao Claude Code, se o perfil quer e se algo
// mudou desde a última montagem dele. Uma por vez: ocupada, tenta de novo
// em meio minuto.
func (s *Servidor) montarComIA(tarefa int64) {
	ctx := context.Background()
	e, perfil, err := s.entradaDaTarefa(ctx, tarefa)
	if err != nil {
		return
	}
	p, err := s.Banco.Perfil(ctx, perfil)
	if err != nil || !p.Montador {
		return
	}
	executavel, err := exec.LookPath(claudecode.Executavel)
	if err != nil {
		return
	}
	base := montagem.Base(e)
	if atual, ok, err := s.Banco.MontagemDaTarefa(ctx, tarefa); err == nil && ok && atual.Origem == montagem.OrigemMontador && atual.Base == base {
		return
	}
	m := &s.montagens
	m.mu.Lock()
	if m.gerando {
		if !m.encerrando {
			m.montador[tarefa] = time.AfterFunc(30*time.Second, func() { m.rodar(func() { s.montarComIA(tarefa) }) })
		}
		m.mu.Unlock()
		return
	}
	m.gerando = true
	m.mu.Unlock()
	defer func() {
		m.mu.Lock()
		m.gerando = false
		m.mu.Unlock()
	}()
	env, err := s.contaDoClaude(ctx, perfil)
	if err != nil {
		return
	}
	s.Avisos.Publicar(perfil, map[string]any{"tipo": "montagem.montando", "tarefa_id": tarefa, "montando": true})
	defer s.Avisos.Publicar(perfil, map[string]any{"tipo": "montagem.montando", "tarefa_id": tarefa, "montando": false})
	resultado, err := montagem.Gerador{Executavel: executavel, Env: env}.Gerar(ctx, e)
	if err != nil {
		log.Printf("tarefa %d: o montador não serviu, fica a montagem simples: %v", tarefa, err)
		return
	}
	s.guardarMontagem(ctx, perfil, resultado, true, true)
}

// montagemDaTarefa lê a montagem guardada ou monta a simples agora, sem
// avisar a tela (ela vai na resposta). Uma de cada vez: o segundo pedido
// pela mesma tarefa encontra a montagem pronta.
func (s *Servidor) montagemDaTarefa(ctx context.Context, tarefa int64) (montagem.Montagem, error) {
	s.montagens.montando.Lock()
	defer s.montagens.montando.Unlock()
	if g, ok, err := s.Banco.MontagemDaTarefa(ctx, tarefa); err != nil {
		return montagem.Montagem{}, err
	} else if ok {
		return montada(g), nil
	}
	return s.montarSimples(tarefa, false, false)
}

// As lousas da reunião

// deckDaReuniao é o deck da daily de hoje ou da sprint, sem recorte.
func (s *Servidor) deckDaReuniao(ctx context.Context, perfil int64, dono dados.DonoLousa) (linha.Deck, error) {
	tipo, consulta := "daily", "/"
	if dono.SprintID != 0 {
		tipo, consulta = "sprint", "/?sprint="+strconv.FormatInt(dono.SprintID, 10)
	}
	r, err := http.NewRequestWithContext(ctx, "GET", consulta, nil)
	if err != nil {
		return linha.Deck{}, err
	}
	return s.montarDeckDe(r, perfil, tipo)
}

// colunasDaReuniao: as tarefas do deck, na ordem da reunião, com a montagem
// de cada uma (guardada, ou a simples montada agora e guardada).
func (s *Servidor) colunasDaReuniao(ctx context.Context, perfil int64, deck linha.Deck) ([]montagem.Coluna, error) {
	ids := []int64{}
	for _, sl := range deck.Slides {
		if !sl.Removida {
			ids = append(ids, sl.TarefaID)
		}
	}
	guardadas, err := s.Banco.MontagensDasTarefas(ctx, ids)
	if err != nil {
		return nil, err
	}
	colunas := []montagem.Coluna{}
	for _, sl := range deck.Slides {
		if sl.Removida {
			continue
		}
		var m montagem.Montagem
		if g, ok := guardadas[sl.TarefaID]; ok {
			m = montada(g)
		} else if m, err = s.montagemDaTarefa(ctx, sl.TarefaID); err != nil {
			continue
		}
		c := montagem.Coluna{Montagem: m, Secao: cmpOr(sl.Secao, sl.Projeto), SecaoID: sl.SecaoID, Imagens: map[int64]montagem.Imagem{}}
		if c.SecaoID == 0 {
			c.SecaoID = sl.ProjetoID
		}
		for _, a := range sl.Anexos {
			c.Imagens[a.ID] = montagem.Imagem{ID: a.ID, Largura: a.Largura, Altura: a.Altura}
		}
		colunas = append(colunas, c)
	}
	return colunas, nil
}

// pecasParaLousa converte o desenho em elementos do montador.
func pecasParaLousa(pecas []montagem.Peca) []dados.NovoDoAgente {
	novos := make([]dados.NovoDoAgente, 0, len(pecas))
	for _, p := range pecas {
		n := dados.NovoDoAgente{Ref: p.Ref, NovoElemento: dados.NovoElemento{Tipo: p.Tipo, Cor: p.Cor, Titulo: p.Titulo, Texto: p.Texto, AnexoID: p.AnexoID, TarefaRef: p.TarefaRef, MontagemDe: p.TarefaID}}
		if p.Tipo == "ligacao" {
			n.De, n.Para = &dados.RefElemento{Ref: p.De}, &dados.RefElemento{Ref: p.Para}
		} else {
			x, y, largura, altura := p.X, p.Y, p.Largura, p.Altura
			n.X, n.Y, n.Largura, n.Altura = &x, &y, &largura, &altura
		}
		novos = append(novos, n)
	}
	return novos
}

// montarLousaDaReuniao redesenha o que é do montador na lousa e avisa as telas.
func (s *Servidor) montarLousaDaReuniao(ctx context.Context, l dados.Lousa) error {
	deck, err := s.deckDaReuniao(ctx, l.PerfilID, l.Dono)
	if err != nil {
		return err
	}
	colunas, err := s.colunasDaReuniao(ctx, l.PerfilID, deck)
	if err != nil {
		return err
	}
	r, err := s.Banco.SubstituirMontador(ctx, l.ID, pecasParaLousa(montagem.Desenhar(colunas)))
	if err != nil {
		return err
	}
	if len(r.Elementos) > 0 || len(r.Removidos) > 0 {
		s.Avisos.Publicar(l.PerfilID, mensagemLousa(l, r.Elementos, r.Removidos))
	}
	return nil
}

// lousasDaReuniao: as lousas da daily de hoje e da sprint atual do perfil
// que já existem.
func (s *Servidor) lousasDaReuniao(ctx context.Context, perfil int64) []dados.Lousa {
	var lista []dados.Lousa
	if l, ok, err := s.Banco.LousaExistente(ctx, dados.DonoLousa{PerfilID: perfil, Dia: hoje()}); err == nil && ok {
		lista = append(lista, l)
	}
	if sprint, err := s.Banco.SprintAtual(ctx, perfil, hoje()); err == nil {
		if l, ok, err := s.Banco.LousaExistente(ctx, dados.DonoLousa{SprintID: sprint.ID}); err == nil && ok {
			lista = append(lista, l)
		}
	}
	return lista
}

func (s *Servidor) remontarLousasDaReuniao(ctx context.Context, perfil int64) {
	for _, l := range s.lousasDaReuniao(ctx, perfil) {
		if err := s.montarLousaDaReuniao(ctx, l); err != nil {
			log.Printf("lousa %d: redesenhando a reunião: %v", l.ID, err)
		}
	}
}

// precisaRedesenhar: a lousa nunca foi montada, ou alguma montagem é mais
// nova do que as peças dela, ou as tarefas mudaram.
func (s *Servidor) precisaRedesenhar(ctx context.Context, l dados.Lousa, deck linha.Deck) bool {
	montadas, err := s.Banco.TarefasMontadas(ctx, l.ID)
	if err != nil {
		return true
	}
	ids := []int64{}
	for _, sl := range deck.Slides {
		if !sl.Removida {
			ids = append(ids, sl.TarefaID)
		}
	}
	if len(montadas) != len(ids) {
		return true
	}
	guardadas, err := s.Banco.MontagensDasTarefas(ctx, ids)
	if err != nil {
		return true
	}
	for _, id := range ids {
		quando, ok := montadas[id]
		if !ok {
			return true
		}
		if g, ok := guardadas[id]; !ok || g.MontadaEm > quando {
			return true
		}
	}
	return false
}

// Rotas

func (s *Servidor) rotasMontagens(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/tarefas/{id}/montagem", s.lerMontagem)
	mux.HandleFunc("POST /v1/tarefas/{id}/montagem", s.remontar)
	mux.HandleFunc("POST /v1/perfis/{id}/daily/lousa", s.abrirLousaDaDaily)
	mux.HandleFunc("POST /v1/sprints/{id}/lousa", s.abrirLousaDaSprint)
}

func (s *Servidor) lerMontagem(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	m, err := s.montagemDaTarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, m)
}

// remontar refaz a montagem simples agora e responde com ela; com
// `com_agente`, o montador roda em seguida (a tela recebe montagem.mudou).
func (s *Servidor) remontar(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		ComAgente bool `json:"com_agente"`
	}
	if r.ContentLength != 0 {
		if err := ler(r, &pedido); err != nil {
			responderErro(w, err)
			return
		}
	}
	_, perfil, err := s.entradaDaTarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	e, _, _ := s.entradaDaTarefa(r.Context(), id)
	m, err := s.guardarMontagem(r.Context(), perfil, montagem.Simples(e), true, true)
	if err != nil {
		responderErro(w, err)
		return
	}
	if pedido.ComAgente {
		s.montagens.mu.Lock()
		s.montagens.perfil[id] = perfil
		if t, ok := s.montagens.montador[id]; ok {
			t.Stop()
		}
		s.montagens.montador[id] = time.AfterFunc(0, func() { s.montagens.rodar(func() { s.montarComIA(id) }) })
		s.montagens.mu.Unlock()
	}
	responderJSON(w, m)
}

// abrirLousaDaReuniao abre (criando se faltar) e, se precisar, redesenha.
func (s *Servidor) abrirLousaDaReuniao(w http.ResponseWriter, r *http.Request, dono dados.DonoLousa) {
	ctx := r.Context()
	l, _, err := s.Banco.AbrirLousa(ctx, dono)
	if err != nil {
		responderErro(w, err)
		return
	}
	deck, err := s.deckDaReuniao(ctx, l.PerfilID, l.Dono)
	if err != nil {
		responderErro(w, err)
		return
	}
	if s.precisaRedesenhar(ctx, l, deck) {
		colunas, err := s.colunasDaReuniao(ctx, l.PerfilID, deck)
		if err != nil {
			responderErro(w, err)
			return
		}
		if _, err := s.Banco.SubstituirMontador(ctx, l.ID, pecasParaLousa(montagem.Desenhar(colunas))); err != nil {
			responderErro(w, err)
			return
		}
	}
	seq := s.Avisos.Seq()
	l, elementos, err := s.Banco.Lousa(ctx, l.ID)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.responderLousa(w, seq, l, elementos)
}

// abrirLousaDaDaily: POST /v1/perfis/{id}/daily/lousa, {"dia": "AAAA-MM-DD"}
// (sem corpo, hoje). Só a de hoje é redesenhada; a de outro dia fica como ficou.
func (s *Servidor) abrirLousaDaDaily(w http.ResponseWriter, r *http.Request) {
	perfil, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Dia string `json:"dia"`
	}
	if r.ContentLength != 0 {
		if err := ler(r, &pedido); err != nil {
			responderErro(w, err)
			return
		}
	}
	dia := cmpOr(pedido.Dia, hoje())
	dono := dados.DonoLousa{PerfilID: perfil, Dia: dia}
	if dia != hoje() {
		seq := s.Avisos.Seq()
		l, elementos, err := s.Banco.AbrirLousa(r.Context(), dono)
		if err != nil {
			responderErro(w, err)
			return
		}
		s.responderLousa(w, seq, l, elementos)
		return
	}
	s.abrirLousaDaReuniao(w, r, dono)
}

// abrirLousaDaSprint: POST /v1/sprints/{id}/lousa.
func (s *Servidor) abrirLousaDaSprint(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.abrirLousaDaReuniao(w, r, dados.DonoLousa{SprintID: id})
}

// agenteMontar: POST /v1/agente/montagem, a pedido do agente da tarefa.
func (s *Servidor) agenteMontar(w http.ResponseWriter, r *http.Request, q quemAgente) {
	s.agendarMontagem(q.perfil, q.tarefa.ID, true)
	responderJSON(w, map[string]any{"tarefa": q.tarefa.ID, "montando": true})
}
