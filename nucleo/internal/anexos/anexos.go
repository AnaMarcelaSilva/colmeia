// Package anexos guarda as imagens anexadas às tarefas (capturas de terminal
// e imagens coladas). Toda imagem é decodificada e codificada de novo: o que
// fica no disco é sempre um PNG válido, sem metadados (um tEXt com caminho ou
// nome de usuário, por exemplo), com o nome tirado do próprio conteúdo.
package anexos

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
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
)

var (
	ErrGrande    = errors.New("imagem grande demais (até 8 MB)")
	ErrDimensoes = fmt.Errorf("imagem grande demais (até %d×%d e 40 milhões de pixels)", MaxLado, MaxLado)
	ErrNaoPNG    = errors.New("a imagem precisa ser um PNG válido")
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

// Caminho do arquivo de uma imagem. O nome nunca vem de quem pede: é o hash.
func Caminho(dir string, sha string) (string, error) {
	if !padraoSha.MatchString(sha) {
		return "", ErrNaoPNG
	}
	return filepath.Join(dir, sha+".png"), nil
}

// Gravar lê um PNG de até MaxBytes, confere as dimensões antes de
// decodificar (contra bombas de descompressão), codifica de novo e grava em
// dir/<sha256>.png com permissão 0600, numa pasta 0700. A mesma imagem duas
// vezes reaproveita o arquivo.
func Gravar(dir string, r io.Reader) (Imagem, error) {
	bruto, err := io.ReadAll(io.LimitReader(r, MaxBytes+1))
	if err != nil {
		return Imagem{}, err
	}
	if len(bruto) > MaxBytes {
		return Imagem{}, ErrGrande
	}
	config, err := png.DecodeConfig(bytes.NewReader(bruto))
	if err != nil {
		return Imagem{}, ErrNaoPNG
	}
	if config.Width <= 0 || config.Height <= 0 || config.Width > MaxLado || config.Height > MaxLado || config.Width*config.Height > MaxPixels {
		return Imagem{}, ErrDimensoes
	}
	imagem, err := png.Decode(bytes.NewReader(bruto))
	if err != nil {
		return Imagem{}, ErrNaoPNG
	}
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
