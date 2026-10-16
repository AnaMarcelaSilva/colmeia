package api

import (
	"context"
	"log"
	"net/http"
	"sync"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/voz"
)

// O ditado por voz: o núcleo baixa e confere o modelo e guarda a opção no
// perfil. Ouvir e transcrever ficam no colmeia-voz, que a tela abre; nada de
// áudio passa por aqui.

// quemBaixa são os perfis que pediram o download em andamento: recebem o
// progresso e ganham o ditado ligado quando o modelo fica pronto.
type quemBaixa struct {
	mu     sync.Mutex
	perfis map[int64]bool
}

func (q *quemBaixa) juntar(perfil int64) {
	q.mu.Lock()
	defer q.mu.Unlock()
	if q.perfis == nil {
		q.perfis = map[int64]bool{}
	}
	q.perfis[perfil] = true
}

// lista devolve os perfis; com esvaziar, o download acabou e a lista zera.
func (q *quemBaixa) lista(esvaziar bool) []int64 {
	q.mu.Lock()
	defer q.mu.Unlock()
	ids := make([]int64, 0, len(q.perfis))
	for id := range q.perfis {
		ids = append(ids, id)
	}
	if esvaziar {
		q.perfis = nil
	}
	return ids
}

func (s *Servidor) iniciarVoz(mux *http.ServeMux) {
	if s.Voz == nil {
		s.Voz = voz.Novo(s.DirDados, voz.Base)
	}
	s.Voz.AoMudar = func(e voz.Estado) {
		for _, perfil := range s.baixandoVoz.lista(false) {
			s.Avisos.Publicar(perfil, map[string]any{"tipo": "voz.modelo", "estado": e.Estado, "baixados": e.Baixados, "tamanho": e.Tamanho, "erro": e.Erro})
		}
	}
	mux.HandleFunc("GET /v1/voz", s.estadoVoz)
	mux.HandleFunc("POST /v1/perfis/{id}/voz/baixar", s.baixarVoz)
	mux.HandleFunc("POST /v1/voz/cancelar", s.cancelarVoz)
	mux.HandleFunc("DELETE /v1/voz/modelo", s.removerVoz)
}

func (s *Servidor) estadoVoz(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, s.Voz.Estado())
}

// baixarVoz começa o download (o tamanho já foi mostrado na tela). Quando o
// modelo fica pronto e conferido, o ditado liga nos perfis que pediram.
func (s *Servidor) baixarVoz(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	s.baixandoVoz.juntar(id)
	err = s.Voz.Baixar(func(err error) {
		perfis := s.baixandoVoz.lista(true)
		if err != nil {
			return
		}
		for _, perfil := range perfis {
			if err := s.Banco.DefinirDitado(context.Background(), perfil, true); err != nil {
				log.Printf("voz: ligar o ditado do perfil %d: %v", perfil, err)
			}
		}
	})
	// Já baixando: este perfil entrou na lista e liga junto.
	if err != nil && err != voz.ErrJaBaixando {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.Voz.Estado())
}

func (s *Servidor) cancelarVoz(w http.ResponseWriter, _ *http.Request) {
	responderJSON(w, map[string]any{"cancelado": s.Voz.Cancelar()})
}

// removerVoz apaga o modelo e desliga o ditado de todos os perfis (sem o
// modelo, nenhum consegue transcrever).
func (s *Servidor) removerVoz(w http.ResponseWriter, r *http.Request) {
	if err := s.Voz.Remover(); err != nil {
		if err == voz.ErrJaBaixando {
			err = dados.ErrInvalido{Motivo: "Espere o download terminar (ou cancele) para apagar o modelo."}
		}
		responderErro(w, err)
		return
	}
	perfis, err := s.Banco.PerfisComDitado(r.Context())
	if err != nil {
		responderErro(w, err)
		return
	}
	for _, perfil := range perfis {
		if err := s.Banco.DefinirDitado(r.Context(), perfil, false); err != nil {
			responderErro(w, err)
			return
		}
	}
	responderJSON(w, s.Voz.Estado())
}

// definirDitado é a parte do PATCH do perfil: ligar exige o modelo no disco.
func (s *Servidor) definirDitado(ctx context.Context, perfil int64, ligado bool) error {
	if ligado && !s.Voz.Pronto() {
		return dados.ErrInvalido{Motivo: "Baixe o modelo de voz antes de ligar o ditado."}
	}
	return s.Banco.DefinirDitado(ctx, perfil, ligado)
}
