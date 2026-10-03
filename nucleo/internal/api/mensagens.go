package api

import (
	"encoding/json"
	"errors"
	"net/http"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// Histórico de mensagens de cada agente (seta para cima na caixa de
// mensagem) e notas da daily e da sprint.
func (s *Servidor) rotasMensagens(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/agentes/{id}/mensagens", s.listarMensagens)
	mux.HandleFunc("POST /v1/agentes/{id}/mensagens", s.guardarMensagem)
	mux.HandleFunc("DELETE /v1/agentes/{id}/mensagens", s.limparMensagens)
	mux.HandleFunc("PUT /v1/tarefas/{id}/notas", s.definirNota)
}

func (s *Servidor) listarMensagens(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, err := s.Banco.ListarMensagens(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, lista)
}

// guardarMensagem guarda o que você mandou ao agente. O envio em si vai pelo
// terminal; aqui só fica o histórico. Um texto que parece ter senha ou chave
// não é guardado: a resposta diz {"guardada": false}.
func (s *Servidor) guardarMensagem(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Texto string `json:"texto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	guardada, err := s.Banco.GuardarMensagem(r.Context(), id, pedido.Texto)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"guardada": guardada})
}

func (s *Servidor) limparMensagens(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	apagadas, err := s.Banco.LimparMensagens(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"apagadas": apagadas})
}

// definirNota grava a nota da tarefa para uma daily ou uma sprint; texto
// vazio apaga.
func (s *Servidor) definirNota(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	// versao (opcional): o atualizada_em da nota que a tela leu ("" se não
	// havia nota). Se ela mudou desde então (o agente complementou), nada é
	// gravado: a resposta é 409 com o texto e a versão atuais, para a tela
	// juntar as duas.
	var pedido struct {
		Tipo    string  `json:"tipo"`
		Periodo string  `json:"periodo"`
		Texto   string  `json:"texto"`
		Versao  *string `json:"versao"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	nota, err := s.Banco.GravarNota(r.Context(), dados.GravacaoNota{Tarefa: id, Tipo: pedido.Tipo, Periodo: pedido.Periodo, Texto: pedido.Texto, Versao: pedido.Versao})
	var mudou dados.ErrNotaMudou
	if errors.As(err, &mudou) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusConflict)
		json.NewEncoder(w).Encode(map[string]any{"erro": mudou.Error(), "texto": mudou.Atual.Texto, "versao": mudou.Atual.AtualizadaEm})
		return
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, nota)
}

// notaDados é o que a mensagem de uma nota precisa do evento gravado.
type notaDados struct {
	Tipo    string `json:"tipo"`
	Periodo string `json:"periodo"`
	Modo    string `json:"modo"`
}
