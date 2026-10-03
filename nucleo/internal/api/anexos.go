package api

import (
	"errors"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strconv"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/anexos"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
)

// anexoDados é o que a mensagem de um anexo precisa do evento gravado.
type anexoDados struct {
	Anexo int64 `json:"anexo"`
}

func (s *Servidor) rotasAnexos(mux *http.ServeMux) {
	mux.HandleFunc("POST /v1/tarefas/{id}/anexos", s.anexarNaTarefa)
	mux.HandleFunc("POST /v1/tarefas/{id}/videos", s.anexarVideo)
	mux.HandleFunc("POST /v1/perfis/{id}/anexos", s.anexarNoPerfil)
	mux.HandleFunc("GET /v1/anexos/{id}", s.lerAnexo)
	mux.HandleFunc("GET /v1/anexos/{id}/info", s.infoAnexo)
	mux.HandleFunc("DELETE /v1/anexos/{id}", s.removerAnexo)
}

func (s *Servidor) pastaAnexos(perfil int64) string {
	return filepath.Join(s.DirDados, "anexos", strconv.FormatInt(perfil, 10))
}

func (s *Servidor) anexarNaTarefa(w http.ResponseWriter, r *http.Request) {
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
	s.anexar(w, r, perfil, id)
}

func (s *Servidor) anexarNoPerfil(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	if _, err := s.Banco.Perfil(r.Context(), id); err != nil {
		responderErro(w, err)
		return
	}
	s.anexar(w, r, id, 0)
}

// anexar recebe um PNG ou um JPEG no corpo (até 8 MB). origem: captura,
// colagem, mensagem ou arquivo; agente (opcional): de qual terminal veio a
// captura; nome (opcional): o nome do arquivo de origem, sem a pasta.
func (s *Servidor) anexar(w http.ResponseWriter, r *http.Request, perfil, tarefa int64) {
	tipo := r.Header.Get("Content-Type")
	if tipo != "image/png" && tipo != "image/jpeg" {
		responderErro(w, dados.ErrInvalido{Motivo: "envie a imagem como image/png ou image/jpeg"})
		return
	}
	q := r.URL.Query()
	novo := dados.NovoAnexo{Perfil: perfil, Tarefa: tarefa, Origem: q.Get("origem"), Legenda: q.Get("legenda"), Nome: q.Get("nome")}
	if v := q.Get("agente"); v != "" {
		agente, err := strconv.ParseInt(v, 10, 64)
		if err != nil || agente <= 0 || tarefa == 0 {
			responderErro(w, dados.ErrInvalido{Motivo: "agente inválido"})
			return
		}
		novo.Agente = agente
	}
	// Confere a origem antes de gravar qualquer coisa no disco.
	if err := validarOrigem(novo.Origem); err != nil {
		responderErro(w, err)
		return
	}
	imagem, err := anexos.GravarImagem(s.pastaAnexos(perfil), r.Body, tipo)
	if errors.Is(err, anexos.ErrGrande) || errors.Is(err, anexos.ErrDimensoes) || errors.Is(err, anexos.ErrNaoPNG) || errors.Is(err, anexos.ErrNaoJPEG) {
		responderErro(w, dados.ErrInvalido{Motivo: err.Error()})
		return
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	novo.Sha256, novo.Largura, novo.Altura, novo.Bytes = imagem.Sha256, imagem.Largura, imagem.Altura, imagem.Bytes
	anexo, err := s.Banco.CriarAnexo(r.Context(), novo)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"id": anexo.ID, "caminho": imagem.Caminho, "largura": anexo.Largura, "altura": anexo.Altura})
}

func validarOrigem(origem string) error {
	for _, o := range dados.OrigensAnexo {
		if o == origem {
			return nil
		}
	}
	return dados.ErrInvalido{Motivo: "origem do anexo inválida: " + strconv.Quote(origem)}
}

func (s *Servidor) lerAnexo(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	anexo, err := s.Banco.Anexo(r.Context(), id)
	if err == nil && anexo.Removido {
		err = dados.ErrNaoEncontrado
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	if anexo.Tipo != "imagem" {
		responderErro(w, dados.ErrInvalido{Motivo: "este anexo é um vídeo: abra no reprodutor do sistema"})
		return
	}
	caminho, err := anexos.Caminho(s.pastaAnexos(anexo.PerfilID), anexo.Sha256, anexo.Formato)
	if err != nil {
		responderErro(w, err)
		return
	}
	conteudo, err := os.ReadFile(caminho)
	if err != nil {
		responderErro(w, dados.ErrNaoEncontrado)
		return
	}
	w.Header().Set("Content-Type", "image/png")
	w.Header().Set("X-Content-Type-Options", "nosniff")
	// O conteúdo de um anexo nunca muda: o nome é o hash.
	w.Header().Set("Cache-Control", "private, max-age=31536000, immutable")
	w.Write(conteudo)
}

func (s *Servidor) removerAnexo(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	anexo, emUso, err := s.Banco.RemoverAnexo(r.Context(), id)
	if err != nil {
		responderErro(w, err)
		return
	}
	if !emUso {
		if caminho, err := anexos.Caminho(s.pastaAnexos(anexo.PerfilID), anexo.Sha256, anexo.Formato); err == nil {
			os.Remove(caminho)
		}
	}
	responderJSON(w, map[string]any{"ok": true})
}

// anexarVideo recebe um vídeo no corpo (mp4, webm, mkv ou mov, até 512 MB),
// copiado em fluxo para o disco. nome (opcional): o nome do arquivo de origem.
func (s *Servidor) anexarVideo(w http.ResponseWriter, r *http.Request) {
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
	tipo := r.Header.Get("Content-Type")
	if _, ok := anexos.TiposDeVideo[tipo]; !ok {
		responderErro(w, dados.ErrInvalido{Motivo: "envie o vídeo como video/mp4, video/webm, video/x-matroska ou video/quicktime"})
		return
	}
	if r.ContentLength > anexos.MaxVideo {
		responderErro(w, dados.ErrInvalido{Motivo: anexos.ErrVideoGrande.Error()})
		return
	}
	nome := r.URL.Query().Get("nome")
	// Confere o nome antes de gravar o arquivo.
	if _, err := dados.NomeDeArquivo(nome); err != nil {
		responderErro(w, err)
		return
	}
	video, err := anexos.GravarVideo(s.pastaAnexos(perfil), r.Body, tipo)
	if errors.Is(err, anexos.ErrVideoGrande) || errors.Is(err, anexos.ErrNaoVideo) || errors.Is(err, anexos.ErrTipoDeVideo) {
		responderErro(w, dados.ErrInvalido{Motivo: err.Error()})
		return
	}
	if err != nil {
		log.Printf("gravando vídeo: %v", err)
		responderErro(w, dados.ErrInvalido{Motivo: "não consegui gravar o vídeo (o disco está cheio?)"})
		return
	}
	anexo, err := s.Banco.CriarAnexo(r.Context(), dados.NovoAnexo{Perfil: perfil, Tarefa: id, Origem: "arquivo", Sha256: video.Sha256,
		Bytes: int(video.Bytes), Tipo: "video", Formato: video.Formato, Nome: nome})
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"id": anexo.ID, "caminho": video.Caminho, "bytes": video.Bytes})
}

// infoAnexo diz o que é o anexo e onde ele está, para a tela abrir um vídeo
// no reprodutor do sistema. O caminho é sempre montado aqui, pelo hash.
func (s *Servidor) infoAnexo(w http.ResponseWriter, r *http.Request) {
	id, err := idDaRota(r)
	if err != nil {
		responderErro(w, err)
		return
	}
	anexo, err := s.Banco.Anexo(r.Context(), id)
	if err == nil && anexo.Removido {
		err = dados.ErrNaoEncontrado
	}
	if err != nil {
		responderErro(w, err)
		return
	}
	caminho, err := anexos.Caminho(s.pastaAnexos(anexo.PerfilID), anexo.Sha256, anexo.Formato)
	if err != nil {
		responderErro(w, err)
		return
	}
	responderJSON(w, map[string]any{"tipo": anexo.Tipo, "formato": anexo.Formato, "nome": anexo.Nome, "bytes": anexo.Bytes,
		"largura": anexo.Largura, "altura": anexo.Altura, "caminho": caminho})
}
