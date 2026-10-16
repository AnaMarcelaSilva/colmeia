package api

import (
	"errors"
	"net/http"
	"strconv"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/arquivos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/ferramentas"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
)

// A aba Arquivos e a aba Descrição da tarefa: só leitura.
func (s *Servidor) rotasCodigo(mux *http.ServeMux) {
	mux.HandleFunc("GET /v1/tarefas/{id}/arquivos/busca", s.buscarNome)
	mux.HandleFunc("GET /v1/tarefas/{id}/arquivos/texto", s.buscarTexto)
	mux.HandleFunc("GET /v1/tarefas/{id}/mudancas", s.listarMudancas)
	mux.HandleFunc("GET /v1/tarefas/{id}/mudancas/arquivo", s.verMudanca)
	mux.HandleFunc("GET /v1/tarefas/{id}/descricao", s.descricaoDaTarefa)
}

// erroDeCodigo: os erros da busca e do diff vão com a mensagem para a tela;
// uma falha do próprio git também (ela diz o que houve), nunca como "erro interno".
func erroDeCodigo(err error) error {
	var invalido dados.ErrInvalido
	switch {
	case arquivos.Erro(err), arquivos.ErroDeCodigo(err):
		return dados.ErrInvalido{Motivo: err.Error()}
	case errors.As(err, &invalido), errors.Is(err, dados.ErrNaoEncontrado):
		return err
	}
	return dados.ErrInvalido{Motivo: "Não consegui consultar o git: " + err.Error()}
}

// buscarNome acha arquivos da pasta da tarefa pelo nome (Ctrl+P).
func (s *Servidor) buscarNome(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	nomes, mais, err := arquivos.BuscarNome(r.Context(), pasta, r.URL.Query().Get("termo"))
	if err != nil {
		responderErro(w, erroDeCodigo(err))
		return
	}
	responderJSON(w, map[string]any{"arquivos": nomes, "mais": mais})
}

// buscarTexto procura um texto no código da pasta da tarefa.
func (s *Servidor) buscarTexto(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	maiusculas, _ := strconv.ParseBool(r.URL.Query().Get("maiusculas"))
	achados, mais, err := arquivos.BuscarTexto(r.Context(), pasta, r.URL.Query().Get("termo"), maiusculas)
	if err != nil {
		responderErro(w, erroDeCodigo(err))
		return
	}
	if achados == nil {
		achados = []git.Achado{}
	}
	responderJSON(w, map[string]any{"achados": achados, "mais": mais})
}

// listarMudancas diz o que mudou na pasta da tarefa contra a branch base.
func (s *Servidor) listarMudancas(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	m, err := arquivos.ListarMudancas(r.Context(), pasta)
	if err != nil {
		responderErro(w, erroDeCodigo(err))
		return
	}
	responderJSON(w, m)
}

// verMudanca devolve o diff de um arquivo contra a branch base.
func (s *Servidor) verMudanca(w http.ResponseWriter, r *http.Request) {
	pasta, err := s.pastaDaTarefa(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	mostrar, _ := strconv.ParseBool(r.URL.Query().Get("mostrar"))
	d, err := arquivos.VerDiferenca(r.Context(), pasta, r.URL.Query().Get("caminho"), mostrar)
	if err != nil {
		responderErro(w, erroDeCodigo(err))
		return
	}
	responderJSON(w, d)
}

// descricaoDaTarefa devolve o que o agente (ou você) escreveu por último
// sobre a tarefa, com quem escreveu e quando.
func (s *Servidor) descricaoDaTarefa(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, _, _, err := s.Banco.Tarefa(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	d, err := s.Banco.DescricaoDaTarefa(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	resposta := map[string]any{"texto": d.Texto, "atualizada_em": d.AtualizadaEm, "tipo": d.Tipo, "periodo": d.Periodo, "agente_id": d.Agente, "autor": "", "quando": quandoLocal(d.AtualizadaEm)}
	switch {
	case d.Texto == "":
	case d.Agente == 0:
		resposta["autor"] = "Você"
	case d.Ferramenta == "":
		resposta["autor"] = "Um agente"
	default:
		autor := ferramentas.Nome(d.Ferramenta)
		if d.Papel != "" {
			autor += " · " + d.Papel
		}
		resposta["autor"] = autor
	}
	responderJSON(w, resposta)
}

// quandoLocal: o momento gravado (UTC) no fuso de quem usa, "16/10 às 14:58".
func quandoLocal(momento string) string {
	t, err := time.Parse(time.RFC3339Nano, momento)
	if err != nil {
		return ""
	}
	return t.Local().Format("02/01 às 15:04")
}
