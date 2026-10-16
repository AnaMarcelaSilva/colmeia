package api

import (
	"context"
	"errors"
	"log"
	"strings"
	"sync"
	"time"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// A frase da reunião: quando a tarefa entra em Revisão ou Concluído, o núcleo
// pede ao agente do Claude Code que está rodando nela uma frase curta e
// falável (pelo mesmo pedido da daily, que ele responde pela nota). Sem
// agente rodando ou sem resposta no prazo, a frase é montada com o título e a
// sua última mensagem. A frase só entra numa nota vazia: o que você escreveu
// nunca é trocado. As instruções do agente já pedem a frase ao terminar; se
// ele gravou, o pedido não sai e a montada fica só como reserva.

// PrazoFrase: quanto o agente tem para responder (os testes encurtam).
var PrazoFrase = 3 * time.Minute

// Tamanho máximo da última mensagem citada na frase montada.
const maxCitacao = 140

type fraseEmCurso struct {
	pedido int64
	perfil int64
	timer  *time.Timer
}

type frases struct {
	mu         sync.Mutex
	emCurso    map[int64]*fraseEmCurso // por tarefa
	encerrando bool
	trabalhos  sync.WaitGroup
}

func (f *frases) iniciar() { f.emCurso = map[int64]*fraseEmCurso{} }

// rodar faz o trabalho fora de quem gravou o evento, e nunca depois do
// encerramento (o banco já pode estar fechando).
func (f *frases) rodar(trabalho func()) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if f.encerrando {
		return
	}
	f.trabalhos.Add(1)
	go func() {
		defer f.trabalhos.Done()
		trabalho()
	}()
}

// doPerfil: as tarefas do perfil esperando a frase do agente agora.
func (f *frases) doPerfil(perfil int64) []int64 {
	f.mu.Lock()
	defer f.mu.Unlock()
	lista := []int64{}
	for tarefa, e := range f.emCurso {
		if e.perfil == perfil {
			lista = append(lista, tarefa)
		}
	}
	return lista
}

func (f *frases) encerrar() {
	f.mu.Lock()
	f.encerrando = true
	for _, e := range f.emCurso {
		e.timer.Stop()
	}
	f.mu.Unlock()
	f.trabalhos.Wait()
}

// aoGravarParaFrase olha os eventos gravados: a tarefa que entrou em Revisão
// ou Concluído pede a frase; o pedido da frase respondido (ou que falhou)
// termina a espera.
func (s *Servidor) aoGravarParaFrase(e dados.Evento, m map[string]any) {
	switch e.Tipo {
	case "tarefa.atualizada":
		t, _ := m["tarefa"].(dados.Tarefa)
		var antes struct {
			ColunaAntes string `json:"coluna_antes"`
		}
		lerDados(e, &antes)
		if (t.Coluna == "revisao" || t.Coluna == "concluido") && antes.ColunaAntes != t.Coluna {
			s.frases.rodar(func() { s.pedirFrase(e.Escopo.Perfil, t.ID) })
		}
	case "pedido.respondido", "pedido.falhou", "pedido.cancelado":
		pedido, _ := m["pedido_id"].(int64)
		tarefa := e.Escopo.Tarefa
		s.frases.mu.Lock()
		em, ok := s.frases.emCurso[tarefa]
		s.frases.mu.Unlock()
		if ok && em.pedido == pedido {
			s.frases.rodar(func() { s.terminarFrase(tarefa) })
		}
	}
}

// textoDoPedidoDeFrase é o que o agente recebe (dentro do pedido da daily).
func textoDoPedidoDeFrase(coluna string) string {
	onde := "Revisão"
	if coluna == "concluido" {
		onde = "Concluído"
	}
	return "A tarefa foi para " + onde + ". Escreva uma frase curta para eu falar na reunião, dizendo o que foi feito " +
		"e o resultado, em linguagem simples, sem termos técnicos (sem nomes de arquivo, função, commit, tabela ou SQL)."
}

func (s *Servidor) pedirFrase(perfil, tarefa int64) {
	ctx := context.Background()
	s.frases.mu.Lock()
	_, ja := s.frases.emCurso[tarefa]
	s.frases.mu.Unlock()
	if ja {
		return
	}
	nota, err := s.Banco.Nota(ctx, tarefa, "daily", hoje())
	if err != nil {
		return
	}
	if strings.TrimSpace(nota.Texto) != "" {
		// O agente já gravou a frase ao terminar (como as instruções pedem):
		// nada de pedir de novo, ela só vai também para a sprint.
		if deAgente, err := s.Banco.NotaDeAgente(ctx, tarefa, "daily", hoje()); err == nil && deAgente {
			s.gravarNaSprint(ctx, perfil, tarefa, nota.Texto)
		}
		return
	}
	t, _, _, err := s.Banco.Tarefa(ctx, tarefa)
	if err != nil {
		return
	}
	agente := s.claudeRodando(ctx, tarefa)
	if agente == 0 {
		s.gravarFraseMontada(ctx, perfil, t)
		return
	}
	p, err := s.Banco.CriarPedido(ctx, tarefa, agente, "daily", hoje(), textoDoPedidoDeFrase(t.Coluna))
	if err != nil {
		log.Printf("tarefa %d: pedindo a frase da reunião: %v", tarefa, err)
		s.gravarFraseMontada(ctx, perfil, t)
		return
	}
	s.frases.mu.Lock()
	if s.frases.encerrando {
		s.frases.mu.Unlock()
		return
	}
	s.frases.emCurso[tarefa] = &fraseEmCurso{pedido: p.ID, perfil: perfil,
		timer: time.AfterFunc(PrazoFrase, func() { s.frases.rodar(func() { s.terminarFrase(tarefa) }) })}
	s.frases.mu.Unlock()
	s.Avisos.Publicar(perfil, map[string]any{"tipo": "tarefa.frase", "tarefa_id": tarefa, "escrevendo": true})
	s.tentarEntregar(agente)
}

// claudeRodando é um Claude Code da tarefa rodando agora (0 se não houver).
func (s *Servidor) claudeRodando(ctx context.Context, tarefa int64) int64 {
	agentes, err := s.Banco.ListarAgentes(ctx, tarefa)
	if err != nil {
		return 0
	}
	for _, a := range agentes {
		if a.Ferramenta == "claude" && s.Agentes.Ativa(a.ID) {
			return a.ID
		}
	}
	return 0
}

// terminarFrase fecha a espera: com a resposta do agente na daily, ela vai
// também para a sprint; sem resposta, entra a frase montada.
func (s *Servidor) terminarFrase(tarefa int64) {
	s.frases.mu.Lock()
	em, ok := s.frases.emCurso[tarefa]
	if ok {
		em.timer.Stop()
		delete(s.frases.emCurso, tarefa)
	}
	s.frases.mu.Unlock()
	if !ok {
		return
	}
	ctx := context.Background()
	defer s.Avisos.Publicar(em.perfil, map[string]any{"tipo": "tarefa.frase", "tarefa_id": tarefa, "escrevendo": false})
	// O pedido sem resposta sai da fila (ou falha, se já foi entregue).
	if p, err := s.Banco.Pedido(ctx, em.pedido); err == nil && p.Aberto() {
		s.Banco.MudarPedido(ctx, p.ID, []string{dados.PedidoFila}, dados.PedidoCancelado, "")
		s.Banco.MudarPedido(ctx, p.ID, []string{dados.PedidoEntregue}, dados.PedidoFalhou, "o agente não respondeu a tempo")
	}
	t, _, _, err := s.Banco.Tarefa(ctx, tarefa)
	if err != nil {
		return
	}
	daily, err := s.Banco.Nota(ctx, tarefa, "daily", hoje())
	if err != nil {
		return
	}
	if strings.TrimSpace(daily.Texto) == "" {
		s.gravarFraseMontada(ctx, em.perfil, t)
		return
	}
	s.gravarNaSprint(ctx, em.perfil, tarefa, daily.Texto)
}

// fraseMontada: "Concluí a tarefa “X”. O último pedido foi: “…”."
func fraseMontada(t dados.Tarefa, ultima string) string {
	frase := "Deixei pronta para revisão a tarefa “" + t.Titulo + "”."
	if t.Coluna == "concluido" {
		frase = "Concluí a tarefa “" + t.Titulo + "”."
	}
	ultima, _, _ = strings.Cut(strings.TrimSpace(ultima), "\n")
	if utf8.RuneCountInString(ultima) > maxCitacao {
		ultima = string([]rune(ultima)[:maxCitacao-1]) + "…"
	}
	if ultima = strings.TrimSpace(ultima); ultima != "" {
		frase += " O último pedido foi: “" + ultima + "”."
	}
	return frase
}

func (s *Servidor) gravarFraseMontada(ctx context.Context, perfil int64, t dados.Tarefa) {
	ultima, _ := s.Banco.UltimaMensagemDaTarefa(ctx, t.ID)
	frase := fraseMontada(t, ultima)
	if !s.gravarSeVazia(ctx, t.ID, "daily", hoje(), frase) && ultima != "" {
		// Recusada (a mensagem parecia ter um segredo): só com o título.
		frase = fraseMontada(t, "")
		if !s.gravarSeVazia(ctx, t.ID, "daily", hoje(), frase) {
			return
		}
	}
	s.gravarNaSprint(ctx, perfil, t.ID, frase)
}

func (s *Servidor) gravarNaSprint(ctx context.Context, perfil, tarefa int64, texto string) {
	sprint, err := s.Banco.SprintAtual(ctx, perfil, hoje())
	if err != nil {
		return
	}
	s.gravarSeVazia(ctx, tarefa, "sprint", sprint.ChaveNota(), texto)
}

// gravarSeVazia grava só se a nota ainda não existe (versão ""): se você
// escreveu nela no meio do caminho, nada muda. Devolve se gravou ou se a
// nota já tinha texto (só o texto recusado volta false).
func (s *Servidor) gravarSeVazia(ctx context.Context, tarefa int64, tipo, periodo, texto string) bool {
	vazia := ""
	_, err := s.Banco.GravarNota(ctx, dados.GravacaoNota{Tarefa: tarefa, Tipo: tipo, Periodo: periodo, Texto: texto, Versao: &vazia})
	var mudou dados.ErrNotaMudou
	if err != nil && !errors.As(err, &mudou) {
		if !errors.As(err, new(dados.ErrInvalido)) {
			log.Printf("tarefa %d: gravando a frase da reunião: %v", tarefa, err)
		}
		return false
	}
	return true
}
