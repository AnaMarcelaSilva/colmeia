package api

import (
	"bytes"
	"encoding/json"
	"errors"
	"io"
	"net/http"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// Lote de operações da lousa: 500 operações com texto de 8.000 caracteres
// cabem folgadas em 1 MB. O limite geral (64 KB) continua para o resto.
const limiteLoteLousa = 1 << 20

// Lousa (quadro livre) do workspace e da tarefa. Mexer na lousa não entra
// na corrente de eventos (decisão 0008): as telas ficam sabendo pelo aviso
// "lousa.mudou", com os elementos inteiros.
func (s *Servidor) rotasLousas(mux *http.ServeMux) {
	mux.HandleFunc("POST /v1/workspaces/{id}/lousa", s.abrirLousaDoWorkspace)
	mux.HandleFunc("POST /v1/tarefas/{id}/lousa", s.abrirLousaDaTarefa)
	mux.HandleFunc("GET /v1/lousas/{id}", s.lerLousa)
	mux.HandleFunc("POST /v1/lousas/{id}/operacoes", s.gravarLousa)
}

// lerAte lê o JSON do corpo, estrito (campo desconhecido é erro), até `limite` bytes.
func lerAte(r *http.Request, destino any, limite int64) error {
	corpo, err := io.ReadAll(io.LimitReader(r.Body, limite+1))
	if err != nil {
		return err
	}
	if int64(len(corpo)) > limite {
		return dados.ErrInvalido{Motivo: "pedido grande demais"}
	}
	decodificador := json.NewDecoder(bytes.NewReader(corpo))
	decodificador.DisallowUnknownFields()
	if err := decodificador.Decode(destino); err != nil {
		return dados.ErrInvalido{Motivo: "pedido mal formado"}
	}
	if decodificador.More() {
		return dados.ErrInvalido{Motivo: "pedido mal formado"}
	}
	return nil
}

// respostaLousa: a lousa, os elementos e o número da última mensagem de
// eventos já incluída (lido antes da consulta, como no retrato do quadro).
func (s *Servidor) responderLousa(w http.ResponseWriter, seq uint64, l dados.Lousa, elementos []dados.Elemento) {
	responderJSON(w, map[string]any{"lousa": l, "elementos": elementos, "seq": seq})
}

func (s *Servidor) abrirLousaDoWorkspace(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	seq := s.Avisos.Seq()
	l, elementos, err := s.Banco.AbrirLousa(r.Context(), dados.DonoLousa{WorkspaceID: id})
	if err != nil {
		responderErro(w, err)
		return
	}
	s.responderLousa(w, seq, l, elementos)
}

func (s *Servidor) abrirLousaDaTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	seq := s.Avisos.Seq()
	l, elementos, err := s.Banco.AbrirLousa(r.Context(), dados.DonoLousa{TarefaID: id})
	if err != nil {
		responderErro(w, err)
		return
	}
	s.responderLousa(w, seq, l, elementos)
}

func (s *Servidor) lerLousa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	seq := s.Avisos.Seq()
	l, elementos, err := s.Banco.Lousa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	s.responderLousa(w, seq, l, elementos)
}

// gravarLousa aplica um lote (tudo ou nada). Se alguma versão não bate,
// responde 409 com o estado atual do que mudou, e nada é gravado.
func (s *Servidor) gravarLousa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Operacoes []dados.Operacao `json:"operacoes"`
	}
	if err := lerAte(r, &pedido, limiteLoteLousa); err != nil {
		responderErro(w, err)
		return
	}
	resultado, err := s.Banco.GravarLousa(r.Context(), id, pedido.Operacoes)
	var mudou dados.ErrLousaMudou
	if errors.As(err, &mudou) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		json.NewEncoder(w).Encode(map[string]any{"erro": mudou.Error(), "elementos": mudou.Elementos, "removidos": mudou.Removidos})
		return
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	s.Avisos.Publicar(resultado.Lousa.PerfilID, mensagemLousa(resultado.Lousa, resultado.Elementos, resultado.Removidos))
	responderJSON(w, resultado)
}

// mensagemLousa é o aviso às telas do que mudou na lousa, com os elementos
// inteiros: aplicar só a versão maior que a local deixa a aplicação
// idempotente e descarta o próprio eco, sem id de cliente.
func mensagemLousa(l dados.Lousa, elementos []dados.Elemento, removidos []int64) map[string]any {
	if removidos == nil {
		removidos = []int64{}
	}
	return map[string]any{"tipo": "lousa.mudou", "lousa_id": l.ID, "dono": l.Dono, "elementos": elementos, "removidos": removidos}
}
