package api

import "net/http"

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
	var pedido struct {
		Tipo    string `json:"tipo"`
		Periodo string `json:"periodo"`
		Texto   string `json:"texto"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	nota, err := s.Banco.DefinirNota(r.Context(), id, pedido.Tipo, pedido.Periodo, pedido.Texto)
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
}
