// Package voz cuida do modelo do ditado por voz: baixa o Whisper de uma
// fonte fixa só quando você liga o ditado, confere o SHA-256 que está no
// código e guarda na pasta de dados (0700, arquivo 0600). Quem transcreve é
// o colmeia-voz, um processo à parte que a tela abre; o núcleo nunca ouve
// nem guarda áudio.
package voz

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"sync"
	"sync/atomic"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
)

// Modelo é um arquivo de modelo com a origem e o conteúdo esperado.
type Modelo struct {
	Nome    string
	URL     string
	SHA256  string
	Tamanho int64
}

// Base é o Whisper base multilíngue (ggml) do repositório oficial do
// whisper.cpp, preso a uma revisão. O hash e o tamanho são os publicados
// pelo Hugging Face para esse arquivo e foram conferidos baixando-o.
var Base = Modelo{
	Nome:    "ggml-base.bin",
	URL:     "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-base.bin",
	SHA256:  "60ed5bc3dd14eea856493d334349b405782ddcaf0028d4b5df4088345fba2efe",
	Tamanho: 147951465,
}

// Estados do modelo.
const (
	Ausente  = "ausente"
	Baixando = "baixando"
	Pronto   = "pronto"
	Falhou   = "erro"
)

// Estado é o que a tela mostra em Ajustes.
type Estado struct {
	Estado   string `json:"estado"`
	Baixados int64  `json:"baixados"`
	Tamanho  int64  `json:"tamanho"`
	Erro     string `json:"erro,omitempty"`
	// Caminho do arquivo, só quando está pronto: a tela passa ao colmeia-voz.
	Caminho string `json:"caminho,omitempty"`
}

var (
	ErrJaBaixando = errors.New("O modelo de voz já está sendo baixado.")
	ErrCancelado  = errors.New("Download cancelado.")
)

// Sem receber nada por este tempo, o download desiste.
const semResposta = 60 * time.Second

type Gerente struct {
	dir    string
	modelo Modelo
	// Cliente é o HTTP usado para baixar (os testes trocam pelo do servidor de teste).
	Cliente *http.Client
	// AoMudar recebe o progresso (no máximo um por ponto percentual) e o fim.
	AoMudar func(Estado)

	mu       sync.Mutex
	cancelar context.CancelFunc
	baixados atomic.Int64
	erro     string
}

// Novo cuida do modelo em <dirDados>/voz.
func Novo(dirDados string, m Modelo) *Gerente {
	return &Gerente{dir: filepath.Join(dirDados, "voz"), modelo: m, Cliente: clientePadrao()}
}

func clientePadrao() *http.Client {
	t := http.DefaultTransport.(*http.Transport).Clone()
	t.TLSHandshakeTimeout = 20 * time.Second
	t.ResponseHeaderTimeout = 30 * time.Second
	return &http.Client{Transport: t, CheckRedirect: soHTTPS}
}

// soHTTPS: o Hugging Face redireciona para a CDN dele; só https e poucas vezes.
func soHTTPS(r *http.Request, via []*http.Request) error {
	if len(via) >= 5 {
		return errors.New("redirecionamentos demais")
	}
	if r.URL.Scheme != "https" {
		return errors.New("redirecionado para fora do https")
	}
	return nil
}

func (g *Gerente) Caminho() string { return filepath.Join(g.dir, g.modelo.Nome) }

// Estado lê o disco: pronto é o arquivo com o tamanho certo (o hash foi
// conferido antes de ele ganhar o nome final).
func (g *Gerente) Estado() Estado {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.estado()
}

func (g *Gerente) estado() Estado {
	e := Estado{Estado: Ausente, Tamanho: g.modelo.Tamanho}
	switch {
	case g.cancelar != nil:
		e.Estado, e.Baixados = Baixando, g.baixados.Load()
	case g.pronto():
		e.Estado, e.Baixados, e.Caminho = Pronto, g.modelo.Tamanho, g.Caminho()
	case g.erro != "":
		e.Estado, e.Erro = Falhou, g.erro
	}
	return e
}

func (g *Gerente) pronto() bool {
	info, err := os.Stat(g.Caminho())
	return err == nil && info.Mode().IsRegular() && info.Size() == g.modelo.Tamanho
}

// Pronto diz se o modelo está no disco.
func (g *Gerente) Pronto() bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.pronto()
}

// Baixar começa o download numa goroutine; aoTerminar recebe nil quando o
// modelo ficou pronto. Com o modelo já pronto, chama aoTerminar(nil) na hora.
func (g *Gerente) Baixar(aoTerminar func(error)) error {
	g.mu.Lock()
	if g.cancelar != nil {
		g.mu.Unlock()
		return ErrJaBaixando
	}
	if g.pronto() {
		g.mu.Unlock()
		aoTerminar(nil)
		return nil
	}
	ctx, cancelar := context.WithCancel(context.Background())
	g.cancelar, g.erro = cancelar, ""
	g.baixados.Store(0)
	g.mu.Unlock()
	g.avisar()
	go func() {
		err := g.baixar(ctx)
		g.mu.Lock()
		g.cancelar = nil
		switch {
		case err == nil:
		case errors.Is(err, ErrCancelado) || ctx.Err() != nil:
			err = ErrCancelado
		default:
			g.erro = err.Error()
		}
		g.mu.Unlock()
		cancelar()
		g.avisar()
		aoTerminar(err)
	}()
	return nil
}

// Cancelar interrompe o download em andamento; o arquivo parcial é apagado.
func (g *Gerente) Cancelar() bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.cancelar == nil {
		return false
	}
	g.cancelar()
	return true
}

// Remover apaga o modelo do disco (não durante o download).
func (g *Gerente) Remover() error {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.cancelar != nil {
		return ErrJaBaixando
	}
	g.erro = ""
	if err := os.Remove(g.Caminho()); err != nil && !errors.Is(err, os.ErrNotExist) {
		return err
	}
	return nil
}

func (g *Gerente) avisar() {
	if g.AoMudar != nil {
		g.AoMudar(g.Estado())
	}
}

func (g *Gerente) baixar(ctx context.Context) (err error) {
	if err := pastaPrivada(g.dir); err != nil {
		return fmt.Errorf("Não consegui criar a pasta do modelo: %w", err)
	}
	tmp, err := os.CreateTemp(g.dir, ".baixando-*")
	if err != nil {
		return fmt.Errorf("Não consegui gravar o modelo: %w", err)
	}
	defer func() {
		tmp.Close()
		if err != nil {
			os.Remove(tmp.Name())
		}
	}()
	if err := tmp.Chmod(0o600); err != nil {
		return err
	}
	if err := g.copiar(ctx, tmp); err != nil {
		return err
	}
	if err := tmp.Sync(); err != nil {
		return err
	}
	if err := tmp.Close(); err != nil {
		return err
	}
	return os.Rename(tmp.Name(), g.Caminho())
}

// copiar baixa para o arquivo conferindo o tamanho e o SHA-256 no caminho.
func (g *Gerente) copiar(ctx context.Context, destino io.Writer) error {
	if !ehHTTPS(g.modelo.URL) {
		return errors.New("A fonte do modelo precisa ser https.")
	}
	// Sem nada chegando por um tempo, desiste (a conexão pode ficar pendurada).
	ctx, cancelar := context.WithCancel(ctx)
	defer cancelar()
	vigia := time.AfterFunc(semResposta, cancelar)
	defer vigia.Stop()

	pedido, err := http.NewRequestWithContext(ctx, http.MethodGet, g.modelo.URL, nil)
	if err != nil {
		return err
	}
	resposta, err := g.Cliente.Do(pedido)
	if err != nil {
		if ctx.Err() != nil {
			return ErrCancelado
		}
		return fmt.Errorf("Não consegui baixar o modelo de voz. Confira a internet e tente de novo. (%v)", err)
	}
	defer resposta.Body.Close()
	if resposta.StatusCode != http.StatusOK {
		return fmt.Errorf("A fonte do modelo respondeu %s.", resposta.Status)
	}
	if resposta.ContentLength >= 0 && resposta.ContentLength != g.modelo.Tamanho {
		return errors.New("A fonte mandou um arquivo de tamanho diferente do esperado; não baixei.")
	}
	hash := sha256.New()
	progresso := &contador{g: g, vigia: vigia}
	n, err := io.Copy(io.MultiWriter(destino, hash, progresso), io.LimitReader(resposta.Body, g.modelo.Tamanho+1))
	if err != nil {
		if ctx.Err() != nil {
			return ErrCancelado
		}
		return fmt.Errorf("O download parou no meio. Tente de novo. (%v)", err)
	}
	if n != g.modelo.Tamanho {
		return errors.New("O arquivo baixado tem tamanho diferente do esperado; apaguei.")
	}
	if hex.EncodeToString(hash.Sum(nil)) != g.modelo.SHA256 {
		return errors.New("O arquivo baixado não confere com o esperado (SHA-256); apaguei.")
	}
	return nil
}

func ehHTTPS(u string) bool { return len(u) > 8 && u[:8] == "https://" }

// contador soma o que chega, segura o vigia e avisa a cada ponto percentual.
type contador struct {
	g      *Gerente
	vigia  *time.Timer
	ultimo int64
}

func (c *contador) Write(p []byte) (int, error) {
	total := c.g.baixados.Add(int64(len(p)))
	c.vigia.Reset(semResposta)
	if passo := c.g.modelo.Tamanho / 100; passo == 0 || total-c.ultimo >= passo {
		c.ultimo = total
		c.g.avisar()
	}
	return len(p), nil
}

func pastaPrivada(dir string) error {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	// 0700 no Unix; no Windows, a lista de acesso só do usuário.
	return protecao.Diretorio(dir)
}
