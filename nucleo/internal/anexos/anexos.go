// Package anexos guarda as imagens e os vídeos anexados às tarefas (capturas
// de terminal, imagens coladas, fotos e vídeos). Toda imagem é decodificada e
// codificada de novo: o que fica no disco é sempre um PNG válido, sem
// metadados (um tEXt com caminho ou nome de usuário, o EXIF e o GPS de uma
// foto), com o nome tirado do próprio conteúdo. Um vídeo não é decodificado:
// os primeiros bytes são conferidos e ele é guardado como veio, para o
// reprodutor do sistema abrir.
package anexos

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"image"
	"image/jpeg"
	"image/png"
	"io"
	"os"
	"path/filepath"
	"regexp"
)

const (
	MaxBytes  = 8 << 20
	MaxLado   = 8192
	MaxPixels = 40_000_000
	// MaxLadoFoto: uma foto maior é reduzida antes de virar PNG (um PNG de
	// 12 megapixels passaria de 20 MB e demoraria a abrir).
	MaxLadoFoto = 3840
	// MaxVideo em bytes.
	MaxVideo = 512 << 20
)

var (
	ErrGrande       = errors.New("imagem grande demais (até 8 MB)")
	ErrDimensoes    = fmt.Errorf("imagem grande demais (até %d×%d e 40 milhões de pixels)", MaxLado, MaxLado)
	ErrNaoPNG       = errors.New("a imagem precisa ser um PNG válido")
	ErrNaoJPEG      = errors.New("a foto precisa ser um JPEG válido")
	ErrVideoGrande  = errors.New("vídeo grande demais (até 512 MB)")
	ErrNaoVideo     = errors.New("o arquivo não é um vídeo mp4, webm, mkv ou mov válido")
	ErrTipoDeVideo  = errors.New("o conteúdo do vídeo não confere com o tipo enviado")
	ErrFormato      = errors.New("formato de anexo desconhecido")
	TiposDeVideo    = map[string]string{"video/mp4": "mp4", "video/webm": "webm", "video/x-matroska": "mkv", "video/quicktime": "mov"}
	formatosValidos = map[string]bool{"png": true, "mp4": true, "webm": true, "mkv": true, "mov": true}
)

// Imagem é o que ficou gravado.
type Imagem struct {
	Sha256  string
	Largura int
	Altura  int
	Bytes   int
	Caminho string
}

var padraoSha = regexp.MustCompile(`^[0-9a-f]{64}$`)

// Caminho do arquivo de um anexo. O nome nunca vem de quem pede: é o hash,
// com uma extensão de uma lista fixa.
func Caminho(dir, sha, formato string) (string, error) {
	if !padraoSha.MatchString(sha) {
		return "", ErrNaoPNG
	}
	if !formatosValidos[formato] {
		return "", ErrFormato
	}
	return filepath.Join(dir, sha+"."+formato), nil
}

// Gravar lê um PNG de até MaxBytes, confere as dimensões antes de
// decodificar (contra bombas de descompressão), codifica de novo e grava em
// dir/<sha256>.png com permissão 0600, numa pasta 0700. A mesma imagem duas
// vezes reaproveita o arquivo.
func Gravar(dir string, r io.Reader) (Imagem, error) {
	return GravarImagem(dir, r, "image/png")
}

// GravarImagem aceita "image/png" ou "image/jpeg". Uma foto JPEG vira PNG:
// o EXIF (com o GPS e o modelo da câmera) fica para trás, e uma foto maior
// que MaxLadoFoto é reduzida.
func GravarImagem(dir string, r io.Reader, tipo string) (Imagem, error) {
	var decodificarConfig func(io.Reader) (image.Config, error)
	var decodificar func(io.Reader) (image.Image, error)
	errFormato := ErrNaoPNG
	switch tipo {
	case "image/png":
		decodificarConfig, decodificar = png.DecodeConfig, png.Decode
	case "image/jpeg":
		decodificarConfig, decodificar, errFormato = jpeg.DecodeConfig, jpeg.Decode, ErrNaoJPEG
	default:
		return Imagem{}, ErrNaoPNG
	}
	bruto, err := io.ReadAll(io.LimitReader(r, MaxBytes+1))
	if err != nil {
		return Imagem{}, err
	}
	if len(bruto) > MaxBytes {
		return Imagem{}, ErrGrande
	}
	config, err := decodificarConfig(bytes.NewReader(bruto))
	if err != nil {
		return Imagem{}, errFormato
	}
	if config.Width <= 0 || config.Height <= 0 || config.Width > MaxLado || config.Height > MaxLado || config.Width*config.Height > MaxPixels {
		return Imagem{}, ErrDimensoes
	}
	imagem, err := decodificar(bytes.NewReader(bruto))
	if err != nil {
		return Imagem{}, errFormato
	}
	if tipo == "image/jpeg" {
		imagem = reduzir(imagem, MaxLadoFoto)
	}
	config.Width, config.Height = imagem.Bounds().Dx(), imagem.Bounds().Dy()
	var limpo bytes.Buffer
	if err := png.Encode(&limpo, imagem); err != nil {
		return Imagem{}, err
	}
	soma := sha256.Sum256(limpo.Bytes())
	sha := hex.EncodeToString(soma[:])
	if err := pastaPrivada(dir); err != nil {
		return Imagem{}, err
	}
	caminho := filepath.Join(dir, sha+".png")
	resultado := Imagem{Sha256: sha, Largura: config.Width, Altura: config.Height, Bytes: limpo.Len(), Caminho: caminho}
	if _, err := os.Stat(caminho); err == nil {
		return resultado, nil
	}
	// Gravado num temporário e trocado de uma vez: ninguém lê pela metade.
	temporario, err := os.CreateTemp(dir, ".anexo-*")
	if err != nil {
		return Imagem{}, err
	}
	defer os.Remove(temporario.Name())
	if err := temporario.Chmod(0o600); err != nil {
		temporario.Close()
		return Imagem{}, err
	}
	if _, err := temporario.Write(limpo.Bytes()); err != nil {
		temporario.Close()
		return Imagem{}, err
	}
	if err := temporario.Close(); err != nil {
		return Imagem{}, err
	}
	return resultado, os.Rename(temporario.Name(), caminho)
}

// pastaPrivada cria a pasta (e a de cima, anexos/) só para o usuário.
func pastaPrivada(dir string) error {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	for _, d := range []string{filepath.Dir(dir), dir} {
		if err := os.Chmod(d, 0o700); err != nil {
			return err
		}
	}
	return nil
}

// reduzir divide a imagem por um fator inteiro (média de cada bloco) até o
// maior lado caber em maximo. Sem dependência nova: só a biblioteca padrão.
func reduzir(img image.Image, maximo int) image.Image {
	b := img.Bounds()
	lado := max(b.Dx(), b.Dy())
	if lado <= maximo {
		return img
	}
	fator := (lado + maximo - 1) / maximo
	largura, altura := max(b.Dx()/fator, 1), max(b.Dy()/fator, 1)
	saida := image.NewNRGBA(image.Rect(0, 0, largura, altura))
	for y := 0; y < altura; y++ {
		for x := 0; x < largura; x++ {
			var r, g, bl, a, n uint32
			for dy := 0; dy < fator; dy++ {
				for dx := 0; dx < fator; dx++ {
					px, py := b.Min.X+x*fator+dx, b.Min.Y+y*fator+dy
					if px >= b.Max.X || py >= b.Max.Y {
						continue
					}
					cr, cg, cb, ca := img.At(px, py).RGBA()
					r, g, bl, a, n = r+cr, g+cg, bl+cb, a+ca, n+1
				}
			}
			i := saida.PixOffset(x, y)
			saida.Pix[i], saida.Pix[i+1], saida.Pix[i+2], saida.Pix[i+3] = uint8(r/n>>8), uint8(g/n>>8), uint8(bl/n>>8), uint8(a/n>>8)
		}
	}
	return saida
}

// Video é o que ficou gravado de um vídeo.
type Video struct {
	Sha256  string
	Formato string
	Bytes   int64
	Caminho string
}

// confereVideo olha os primeiros bytes: "ftyp" no deslocamento 4 para mp4 e
// mov (ou um átomo do QuickTime antigo, no mov) e o cabeçalho EBML para webm e
// mkv. A extensão vem do tipo enviado, e os dois precisam bater: assim o
// reprodutor do sistema nunca recebe outra coisa com nome de vídeo.
func confereVideo(formato string, inicio []byte) error {
	if len(inicio) < 12 {
		return ErrNaoVideo
	}
	ebml := bytes.Equal(inicio[:4], []byte{0x1a, 0x45, 0xdf, 0xa3})
	atomo := string(inicio[4:8])
	iso := atomo == "ftyp"
	quicktime := iso || atomo == "moov" || atomo == "mdat" || atomo == "wide" || atomo == "free" || atomo == "skip"
	ok := map[string]bool{"mp4": iso, "mov": quicktime, "webm": ebml, "mkv": ebml}[formato]
	if ok {
		return nil
	}
	if ebml || quicktime {
		return ErrTipoDeVideo
	}
	return ErrNaoVideo
}

// GravarVideo copia o vídeo (até MaxVideo) direto para um temporário 0600
// na pasta, calculando o sha256 durante a cópia, e troca pelo nome final de
// uma vez. Em qualquer erro (inclusive disco cheio) o temporário é apagado.
func GravarVideo(dir string, r io.Reader, tipo string) (Video, error) {
	formato, ok := TiposDeVideo[tipo]
	if !ok {
		return Video{}, ErrNaoVideo
	}
	if err := pastaPrivada(dir); err != nil {
		return Video{}, err
	}
	temporario, err := os.CreateTemp(dir, ".video-*")
	if err != nil {
		return Video{}, err
	}
	defer os.Remove(temporario.Name())
	defer temporario.Close()
	if err := temporario.Chmod(0o600); err != nil {
		return Video{}, err
	}
	inicio := make([]byte, 12)
	lidos, err := io.ReadFull(r, inicio)
	if err != nil && !errors.Is(err, io.ErrUnexpectedEOF) && !errors.Is(err, io.EOF) {
		return Video{}, err
	}
	if err := confereVideo(formato, inicio[:lidos]); err != nil {
		return Video{}, err
	}
	soma := sha256.New()
	destino := io.MultiWriter(temporario, soma)
	if _, err := destino.Write(inicio); err != nil {
		return Video{}, err
	}
	copiados, err := io.Copy(destino, io.LimitReader(r, MaxVideo+1-int64(lidos)))
	if err != nil {
		return Video{}, err
	}
	total := copiados + int64(lidos)
	if total > MaxVideo {
		return Video{}, ErrVideoGrande
	}
	if err := temporario.Close(); err != nil {
		return Video{}, err
	}
	sha := hex.EncodeToString(soma.Sum(nil))
	caminho := filepath.Join(dir, sha+"."+formato)
	video := Video{Sha256: sha, Formato: formato, Bytes: total, Caminho: caminho}
	if _, err := os.Stat(caminho); err == nil {
		return video, nil
	}
	return video, os.Rename(temporario.Name(), caminho)
}
