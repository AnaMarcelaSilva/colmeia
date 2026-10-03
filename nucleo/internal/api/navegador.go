package api

import (
	"net/http"
	"strconv"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/arquivos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/navegador"
)

func (s *Servidor) rotasNavegador(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/navegador", s.infoNavegador)
	mux.HandleFunc("GET /v1/tarefas/{id}/navegador", s.navegadorDaTarefa)
	mux.HandleFunc("POST /v1/tarefas/{id}/navegador", s.abrirNavegadorDaTarefa)
	mux.HandleFunc("DELETE /v1/tarefas/{id}/navegador", s.fecharNavegadorDaTarefa)
	mux.HandleFunc("POST /v1/tarefas/{id}/navegador/captura", s.capturarNavegadorDaTarefa)
	mux.HandleFunc("PUT /v1/perfis/{id}/apresentando", s.definirApresentando)
}

// definirApresentando: a tela avisa que passou (ou deixou de) mostrar algo
// que pode estar compartilhado: a apresentação, a Daily ou a Sprint.
// Enquanto isso, o navegador que o agente abre fica fora da tela, sem cobrir
// o que está sendo mostrado. A geometria (opcional) é onde a janela abre ao
// lado da Colmeia; uma geometria pequena demais é ignorada.
func (s *Servidor) definirApresentando(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Ativo     bool                 `json:"ativo"`
		Geometria *navegador.Geometria `json:"geometria"`
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	if pedido.Geometria != nil && !pedido.Geometria.Valida() {
		responderErro(w, dados.ErrInvalido{Motivo: "geometria da janela inválida"})
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	s.Navegadores.Apresentando(id, pedido.Ativo)
	if pedido.Geometria != nil {
		s.Navegadores.Lembrar(id, *pedido.Geometria)
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) rotasArquivos(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/tarefas/{id}/arquivos", s.listarArquivos)
	mux.HandleFunc("GET /v1/tarefas/{id}/arquivo", s.verArquivo)
	mux.HandleFunc("GET /v1/tarefas/{id}/arquivo/imagem", s.imagemDoArquivo)
}

// infoNavegador diz se há um Chrome ou Chromium para a Colmeia usar.
func (s *Servidor) infoNavegador(w http.ResponseWriter, _ *http.Request) {
	_, nome, err := navegador.Encontrar()
	resposta := map[string]any{"instalado": err == nil, "nome": nome}
	if err != nil {
		resposta["motivo"] = err.Error()
	}
	responderJSON(w, resposta)
}

func (s *Servidor) navegadorDaTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	_, _, perfil, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.Navegadores.Aberto(perfil, id))
}

// abrirNavegadorDaTarefa abre a janela da tarefa ao lado da Colmeia (ou traz
// para frente) e, com url, vai para o endereço.
func (s *Servidor) abrirNavegadorDaTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		URL                   string `json:"url"`
		X, Y, Largura, Altura int
	}
	if err := ler(r, &pedido); err != nil {
		responderErro(w, err)
		return
	}
	t, p, perfil, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	var geo *navegador.Geometria
	if pedido.Largura != 0 || pedido.Altura != 0 {
		g := navegador.Geometria{X: pedido.X, Y: pedido.Y, Largura: pedido.Largura, Altura: pedido.Altura}
		if !g.Valida() {
			responderErro(w, dados.ErrInvalido{Motivo: "geometria da janela inválida"})
			return
		}
		geo = &g
	}
	if _, err := s.abrirNavegador(r.Context(), perfil, id, t.Pasta(p), pedido.URL, geo, false, 0); err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, s.Navegadores.Aberto(perfil, id))
}

func (s *Servidor) fecharNavegadorDaTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	_, _, perfil, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	if err := s.Navegadores.Fechar(r.Context(), perfil, id); err != nil && err != navegador.ErrFechado {
		responderErro(w, dados.ErrInvalido{Motivo: "não consegui fechar o navegador: " + err.Error()})
		return
	}
	responderJSON(w, map[string]any{"ok": true})
}

func (s *Servidor) capturarNavegadorDaTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	var pedido struct {
		Legenda string `json:"legenda"`
	}
	if r.ContentLength != 0 {
		if err := ler(r, &pedido); err != nil {
			responderErro(w, err)
			return
		}
	}
	_, _, perfil, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	_, anexo, err := s.capturarNavegador(r.Context(), perfil, id, 0, true, pedido.Legenda)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"id": anexo})
}

// pastaDaTarefa devolve a pasta onde os agentes da tarefa trabalham.
func (s *Servidor) pastaDaTarefa(r *http.Request) (string, error) {
	id, err := idDaRota(r)
	if err != nil {
		return "", err
	}
	t, p, _, err := s.Banco.Tarefa(r.Context(), id)
	if err != nil {
		return "", err
	}
	return t.Pasta(p), nil
}

// listarArquivos lista um nível da pasta da tarefa (só leitura).
func (s *Servidor) listarArquivos(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	lista, mais, err := arquivos.Listar(pasta, r.URL.Query().Get("caminho"))
	if err != nil {
		responderErro(w, erroDeArquivo(err))
		return
	}
	responderJSON(w, map[string]any{"pasta": pasta, "entradas": lista, "mais": mais})
}

// verArquivo devolve a pré-visualização de um arquivo da pasta da tarefa.
func (s *Servidor) verArquivo(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	mostrar, _ := strconv.ParseBool(r.URL.Query().Get("mostrar"))
	previa, err := arquivos.Ver(pasta, r.URL.Query().Get("caminho"), mostrar)
	if err != nil {
		responderErro(w, erroDeArquivo(err))
		return
	}
	responderJSON(w, previa)
}

// imagemDoArquivo devolve a imagem do arquivo como PNG reduzido.
func (s *Servidor) imagemDoArquivo(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	png, err := arquivos.Imagem(pasta, r.URL.Query().Get("caminho"), arquivos.LadoPrevia)
	if err != nil {
		responderErro(w, erroDeArquivo(err))
		return
	}
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	w.Write(png)
}
