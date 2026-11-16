package api

import (
	"context"
	"encoding/json"
	"log"
	"net/http"
	"path/filepath"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/atualizacao"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/avisos"
)

// A versão nova: a tela pede para procurar ao abrir, o núcleo olha a release
// (uma vez por hora no máximo) e avisa pelo evento "atualizacao". Instalar
// agora reinicia o núcleo e fecha os agentes e o Play; "quando fecharem"
// espera não sobrar nenhum rodando.

// Tempo para o evento "reiniciando" chegar às telas antes de o núcleo sair.
const antesDeReiniciar = 300 * time.Millisecond

// estadoAtualizacao é o estado do gerente com o que só o núcleo sabe.
type estadoAtualizacao struct {
	atualizacao.Estado
	// Abertos são os agentes e o Play rodando, que reiniciar fecha.
	Abertos  int  `json:"abertos"`
	AoFechar bool `json:"ao_fechar"`
}

func (s *Servidor) iniciarAtualizacao(mux *http.ServeMux) {
	if s.Atualizacao == nil {
		dirBin := ""
		if s.Executavel != "" {
			dirBin = filepath.Dir(s.Executavel)
		}
		s.Atualizacao = atualizacao.Novo(s.Versao, s.DirDados, dirBin)
	}
	s.Atualizacao.CopiarBanco = func(destino string) error { return s.Banco.CopiarPara(context.Background(), destino) }
	s.Atualizacao.AoMudar = func(e atualizacao.Estado) { s.publicarAtualizacao(e) }
	mux.HandleFunc("GET /v1/atualizacao", s.verAtualizacao)
	mux.HandleFunc("POST /v1/atualizacao/procurar", s.procurarAtualizacao)
	mux.HandleFunc("POST /v1/atualizacao/instalar", s.instalarAtualizacao)
	mux.HandleFunc("POST /v1/atualizacao/cancelar", s.cancelarAtualizacao)
	mux.HandleFunc("PUT /v1/atualizacao/opcoes", s.opcoesAtualizacao)
}

func (s *Servidor) abertos() int { return s.Agentes.Rodando() + s.Execucoes.Rodando() }

func (s *Servidor) comAbertos(e atualizacao.Estado) estadoAtualizacao {
	return estadoAtualizacao{Estado: e, Abertos: s.abertos(), AoFechar: s.atualizarAoFechar.Load()}
}

func (s *Servidor) publicarAtualizacao(e atualizacao.Estado) {
	e.Aviso = ""
	bruto, _ := json.Marshal(s.comAbertos(e))
	var m map[string]any
	json.Unmarshal(bruto, &m)
	m["tipo"] = "atualizacao"
	s.Avisos.Publicar(avisos.Todos, m)
}

func (s *Servidor) verAtualizacao(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, s.comAbertos(s.Atualizacao.Estado()))
}

// procurarAtualizacao responde já com o que se sabe (e o aviso da última
// troca, uma vez); o que a rede disser chega pelo evento.
func (s *Servidor) procurarAtualizacao(w http.ResponseWriter, _ *http.Request) {
	go s.Atualizacao.Procurar(context.Background())
	e := s.comAbertos(s.Atualizacao.Estado())
	e.Aviso = s.Atualizacao.VerAviso()
	responderJSON(w, e)
}

func (s *Servidor) instalarAtualizacao(w http.ResponseWriter, r *http.Request) {
	var pedido struct {
		Quando string `json:"quando"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Quando != "agora" && pedido.Quando != "ao_fechar" {
		http.Error(w, `quando precisa ser "agora" ou "ao_fechar"`, http.StatusBadRequest)
		return
	}
	s.atualizarAoFechar.Store(pedido.Quando == "ao_fechar")
	go func() {
		if err := s.Atualizacao.Preparar(context.Background()); err != nil {
			s.atualizarAoFechar.Store(false)
			log.Printf("atualização: %v", err)
			return
		}
		if pedido.Quando == "agora" {
			s.aplicarAtualizacao()
			return
		}
		s.atualizarSeFecharam()
		s.publicarAtualizacao(s.Atualizacao.Estado())
	}()
	w.WriteHeader(http.StatusAccepted)
}

func (s *Servidor) cancelarAtualizacao(w http.ResponseWriter, _ *http.Request) {
	s.atualizarAoFechar.Store(false)
	s.Atualizacao.Cancelar()
	s.publicarAtualizacao(s.Atualizacao.Estado())
	w.WriteHeader(http.StatusNoContent)
}

func (s *Servidor) opcoesAtualizacao(w http.ResponseWriter, r *http.Request) {
	var o atualizacao.Opcoes
	if err := ler(r, &o); err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Atualizacao.GravarOpcoes(o); err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	w.WriteHeader(http.StatusNoContent)
}

// atualizarSeFecharam aplica a versão pronta quando foi pedida para depois
// e não sobrou nada rodando. Chamado no fim de cada agente e do Play.
func (s *Servidor) atualizarSeFecharam() {
	if s.Atualizacao == nil || !s.atualizarAoFechar.Load() || s.abertos() > 0 {
		return
	}
	if s.Atualizacao.Estado().Estado == atualizacao.Pronta {
		s.aplicarAtualizacao()
	}
}

func (s *Servidor) aplicarAtualizacao() {
	s.atualizarAoFechar.Store(false)
	if s.AoEncerrar == nil {
		return
	}
	if err := s.Atualizacao.Aplicar(s.Avisos.Telas() > 0); err != nil {
		log.Printf("atualização: %v", err)
		return
	}
	time.AfterFunc(antesDeReiniciar, s.AoEncerrar)
}
