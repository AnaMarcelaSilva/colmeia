package api

import (
	"encoding/json"
	"net/http"
	"path/filepath"
	"strings"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/uso"
)

// O uso dos agentes do Claude Code: a statusLine de cada um chama
// "colmeia-nucleo uso", que manda aqui o contexto da conversa e os limites
// da conta. Fica só em memória: é um retrato do agora, não histórico.

// limiteUso: o corpo do POST /v1/agente/uso tem poucas dezenas de bytes.
const limiteUso = 4 << 10

type usoDoAgente struct {
	perfil, tarefa int64
	contexto       uso.Contexto
}

// usoDaConta: os limites valem para a conta, e a conta do Claude Code é uma
// por perfil (a do sistema ou a separada).
type usoDaConta struct {
	cincoHoras, semana *uso.Limite
}

type memoriaUso struct {
	mu      sync.Mutex
	agentes map[int64]usoDoAgente
	contas  map[int64]usoDaConta
}

func (m *memoriaUso) iniciar() {
	m.agentes, m.contas = map[int64]usoDoAgente{}, map[int64]usoDaConta{}
}

// guardar devolve se algo mudou. Um limite que não veio fica como estava:
// um agente recém-aberto ainda não sabe os limites da conta.
func (m *memoriaUso) guardar(agente, perfil, tarefa int64, u uso.Uso) bool {
	m.mu.Lock()
	defer m.mu.Unlock()
	mudou := false
	antes, tinha := m.agentes[agente]
	switch {
	case u.Contexto != nil && (!tinha || antes.contexto != *u.Contexto):
		m.agentes[agente] = usoDoAgente{perfil, tarefa, *u.Contexto}
		mudou = true
	case u.Contexto == nil && tinha:
		delete(m.agentes, agente)
		mudou = true
	}
	conta := m.contas[perfil]
	for _, par := range []struct{ novo, guardado **uso.Limite }{{&u.CincoHoras, &conta.cincoHoras}, {&u.Semana, &conta.semana}} {
		if *par.novo != nil && (*par.guardado == nil || **par.guardado != **par.novo) {
			*par.guardado = *par.novo
			mudou = true
		}
	}
	m.contas[perfil] = conta
	return mudou
}

func (m *memoriaUso) esquecer(agente int64) {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.agentes, agente)
}

// limiteParaTela leva, além da hora Unix, o texto da dica ("volta às 14:30").
type limiteParaTela struct {
	uso.Limite
	VoltaTexto string `json:"volta_texto"`
}

func paraTela(l *uso.Limite, agora time.Time) *limiteParaTela {
	if l == nil || time.Unix(l.Volta, 0).Before(agora) {
		return nil
	}
	return &limiteParaTela{Limite: *l, VoltaTexto: textoDaVolta(time.Unix(l.Volta, 0), agora)}
}

// textoDaVolta: "volta às 14:30" no mesmo dia; senão com a data.
func textoDaVolta(volta, agora time.Time) string {
	volta, agora = volta.Local(), agora.Local()
	if volta.Format("2006-01-02") == agora.Format("2006-01-02") {
		return "volta às " + volta.Format("15:04")
	}
	return "volta às " + volta.Format("15:04") + " de " + volta.Format("02/01")
}

type contextoParaTela struct {
	AgenteID int64        `json:"agente_id"`
	TarefaID int64        `json:"tarefa_id"`
	Contexto uso.Contexto `json:"contexto"`
}

// usoParaTela é o uso de um perfil: o contexto de cada agente e a conta.
type usoParaTela struct {
	Agentes    []contextoParaTela `json:"agentes"`
	CincoHoras *limiteParaTela    `json:"cinco_horas,omitempty"`
	Semana     *limiteParaTela    `json:"semana,omitempty"`
}

func (m *memoriaUso) doPerfil(perfil int64) usoParaTela {
	m.mu.Lock()
	defer m.mu.Unlock()
	agora := time.Now()
	r := usoParaTela{Agentes: []contextoParaTela{}}
	for id, a := range m.agentes {
		if a.perfil == perfil {
			r.Agentes = append(r.Agentes, contextoParaTela{id, a.tarefa, a.contexto})
		}
	}
	conta := m.contas[perfil]
	r.CincoHoras, r.Semana = paraTela(conta.cincoHoras, agora), paraTela(conta.semana, agora)
	return r
}

// agenteUso recebe o uso que a statusLine do agente leu. Não passa pelo
// banco: o agente sai do token e a tarefa, da memória do agente rodando.
func (s *Servidor) agenteUso(w http.ResponseWriter, r *http.Request) {
	quem, ok := canal.QuemPediu(r.Context())
	c, rodando := s.contexto(quem.Agente)
	if !ok || quem.Agente == 0 || !rodando {
		http.Error(w, "não autorizado", http.StatusUnauthorized)
		return
	}
	var u uso.Uso
	if err := lerAte(r, &u, limiteUso); err != nil {
		responderErro(w, err)
		return
	}
	if err := u.Validar(); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}
	if s.uso.guardar(c.ID, c.Perfil, c.TarefaID, u) {
		m := map[string]any{"tipo": "agente.uso", "agente_id": c.ID, "tarefa_id": c.TarefaID}
		if u.Contexto != nil {
			m["contexto"] = u.Contexto
		}
		conta := s.uso.doPerfil(c.Perfil)
		if conta.CincoHoras != nil {
			m["cinco_horas"] = conta.CincoHoras
		}
		if conta.Semana != nil {
			m["semana"] = conta.Semana
		}
		s.Avisos.Publicar(c.Perfil, m)
	}
	w.WriteHeader(http.StatusNoContent)
}

// aspasShell protege um caminho para a linha de comando da statusLine, que
// o Claude Code roda pelo shell.
func aspasShell(s string) string {
	return "'" + strings.ReplaceAll(s, "'", `'\''`) + "'"
}

// configuracaoUso é o arquivo de settings da sessão (--settings): só a
// statusLine, que chama o próprio núcleo com o arquivo do token. O
// ~/.claude de quem usa não é tocado.
func (s *Servidor) configuracaoUso(arquivoToken string) []byte {
	comando := aspasShell(s.Executavel) + " uso --socket " + aspasShell(filepath.Join(s.DirCanal, canal.NomeSocket)) +
		" --token-arquivo " + aspasShell(arquivoToken)
	bruto, _ := json.MarshalIndent(map[string]any{"statusLine": map[string]any{"type": "command", "command": comando, "padding": 0}}, "", "  ")
	return bruto
}
