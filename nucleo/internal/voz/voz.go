// Package voz cuida dos modelos de voz: baixa o Whisper (o ditado e os
// comandos falados) e a voz da abelha (Kokoro, a voz e o ONNX Runtime) de
// fontes fixas só quando você liga cada item, confere o SHA-256 que está no
// código e guarda na pasta de dados (0700, arquivos 0600). Quem ouve,
// transcreve e fala é o colmeia-voz, um processo à parte que a tela abre; o
// núcleo nunca ouve nem guarda áudio.
package voz

import (
	"archive/tar"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"hash"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
)

// Modelo é um arquivo com a origem e o conteúdo esperado.
type Modelo struct {
	Nome    string
	URL     string
	SHA256  string
	Tamanho int64
	// Extrair: o que vem da URL é um .tgz e só este membro dele fica no
	// disco, com o nome Nome, conferido por TamanhoFinal e SHA256Final.
	Extrair      string
	TamanhoFinal int64
	SHA256Final  string
}

// noDisco é o tamanho do arquivo depois de pronto.
func (m Modelo) noDisco() int64 {
	if m.Extrair != "" {
		return m.TamanhoFinal
	}
	return m.Tamanho
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

// Small é o Whisper small multilíngue, do mesmo repositório e revisão: mais
// preciso em português que o base, três vezes maior e mais lento. Hash e
// tamanho publicados pelo Hugging Face (o oid do LFS) e conferidos baixando-o.
var Small = Modelo{
	Nome:    "ggml-small.bin",
	URL:     "https://huggingface.co/ggerganov/whisper.cpp/resolve/5359861c739e955e79d9a303bcbc70fb988958b1/ggml-small.bin",
	SHA256:  "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b",
	Tamanho: 487601967,
}

// Kokoro é a voz da abelha: o Kokoro-82M (v1.0, Apache-2.0) quantizado em
// 8 bits pela comunidade ONNX, a voz feminina pt-BR pf_dora e o ONNX Runtime
// (MIT) da Microsoft, que roda o modelo. Os dois primeiros presos a uma
// revisão do Hugging Face; o runtime, a uma versão do GitHub, com o hash do
// pacote (o mesmo publicado pelo GitHub) e da biblioteca que sai dele.
var Kokoro = []Modelo{
	{
		Nome:    "kokoro-v1.0-q8.onnx",
		URL:     "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231/onnx/model_quantized.onnx",
		SHA256:  "fbae9257e1e05ffc727e951ef9b9c98418e6d79f1c9b6b13bd59f5c9028a1478",
		Tamanho: 92361116,
	},
	{
		Nome:    "pf_dora.bin",
		URL:     "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231/voices/pf_dora.bin",
		SHA256:  "3da7b5b2d91847ebf5646f57631af6ececae3c29a89cd300f06edf9aa6cfe9ee",
		Tamanho: 522240,
	},
	{
		Nome:         "libonnxruntime.so.1.31.0",
		URL:          "https://github.com/microsoft/onnxruntime/releases/download/v1.31.0/onnxruntime-linux-x64-1.31.0.tgz",
		SHA256:       "cc5c72baf5ae5c8238a6841f0897227be2d02826b9cf98eaf02fdefaeeb03a57",
		Tamanho:      11408003,
		Extrair:      "onnxruntime-linux-x64-1.31.0/lib/libonnxruntime.so.1.31.0",
		TamanhoFinal: 29345384,
		SHA256Final:  "469849977cb60744ea160927e1a3a0643fd4c55c0aff743251c6d293402e334e",
	},
}

// KokoroDisponivel: o runtime fixado acima é o do Linux x86-64. Nos outros
// sistemas a voz da abelha ainda não tem um pacote conferido.
func KokoroDisponivel() bool { return runtime.GOOS == "linux" && runtime.GOARCH == "amd64" }

// Estados do modelo.
const (
	Ausente  = "ausente"
	Baixando = "baixando"
	Pronto   = "pronto"
	Falhou   = "erro"
	// Indisponivel: não há modelo conferido para este sistema.
	Indisponivel = "indisponivel"
)

// Estado é o que a tela mostra em Ajustes.
type Estado struct {
	Estado   string `json:"estado"`
	Baixados int64  `json:"baixados"`
	Tamanho  int64  `json:"tamanho"`
	Erro     string `json:"erro,omitempty"`
	// Caminho do (primeiro) arquivo, só quando está pronto: a tela passa ao colmeia-voz.
	Caminho string `json:"caminho,omitempty"`
	// Caminhos de todos os arquivos, na ordem, quando está pronto.
	Caminhos []string `json:"caminhos,omitempty"`
}

var (
	ErrJaBaixando = errors.New("O modelo de voz já está sendo baixado.")
	ErrCancelado  = errors.New("Download cancelado.")
)

// Sem receber nada por este tempo, o download desiste.
const semResposta = 60 * time.Second

type Gerente struct {
	dir      string
	arquivos []Modelo
	// Cliente é o HTTP usado para baixar (os testes trocam pelo do servidor de teste).
	Cliente *http.Client
	// AoMudar recebe o progresso (no máximo um por ponto percentual) e o fim.
	AoMudar func(Estado)

	mu       sync.Mutex
	cancelar context.CancelFunc
	baixados atomic.Int64
	erro     string
}

// Novo cuida dos arquivos em <dirDados>/voz (um modelo pode ter vários).
func Novo(dirDados string, arquivos ...Modelo) *Gerente {
	return &Gerente{dir: filepath.Join(dirDados, "voz"), arquivos: arquivos, Cliente: clientePadrao()}
}

// Pasta é onde os modelos ficam (e as gravações do nome da abelha).
func (g *Gerente) Pasta() string { return g.dir }

// total é o que se baixa da rede para ter tudo.
func (g *Gerente) total() int64 {
	var t int64
	for _, m := range g.arquivos {
		t += m.Tamanho
	}
	return t
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

// Caminho é o do primeiro arquivo (o único, no Whisper).
func (g *Gerente) Caminho() string { return filepath.Join(g.dir, g.arquivos[0].Nome) }

func (g *Gerente) caminhos() []string {
	c := make([]string, len(g.arquivos))
	for i, m := range g.arquivos {
		c[i] = filepath.Join(g.dir, m.Nome)
	}
	return c
}

// Estado lê o disco: pronto é cada arquivo com o tamanho certo (o hash foi
// conferido antes de ele ganhar o nome final).
func (g *Gerente) Estado() Estado {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.estado()
}

func (g *Gerente) estado() Estado {
	e := Estado{Estado: Ausente, Tamanho: g.total()}
	switch {
	case g.cancelar != nil:
		e.Estado, e.Baixados = Baixando, g.baixados.Load()
	case g.pronto():
		e.Estado, e.Baixados, e.Caminho, e.Caminhos = Pronto, e.Tamanho, g.Caminho(), g.caminhos()
	case g.erro != "":
		e.Estado, e.Erro = Falhou, g.erro
	}
	return e
}

func (g *Gerente) pronto() bool {
	for _, m := range g.arquivos {
		if !g.temPronto(m) {
			return false
		}
	}
	return true
}

func (g *Gerente) temPronto(m Modelo) bool {
	info, err := os.Stat(filepath.Join(g.dir, m.Nome))
	return err == nil && info.Mode().IsRegular() && info.Size() == m.noDisco()
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

// Remover apaga os arquivos do disco (não durante o download).
func (g *Gerente) Remover() error {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.cancelar != nil {
		return ErrJaBaixando
	}
	g.erro = ""
	for _, c := range g.caminhos() {
		if err := os.Remove(c); err != nil && !errors.Is(err, os.ErrNotExist) {
			return err
		}
	}
	return nil
}

func (g *Gerente) avisar() {
	if g.AoMudar != nil {
		g.AoMudar(g.Estado())
	}
}

// baixar traz, um por vez, os arquivos que ainda não estão prontos. O
// progresso soma todos; os que já estavam contam como baixados.
func (g *Gerente) baixar(ctx context.Context) error {
	if err := pastaPrivada(g.dir); err != nil {
		return fmt.Errorf("Não consegui criar a pasta do modelo: %w", err)
	}
	for _, m := range g.arquivos {
		if g.temPronto(m) {
			g.baixados.Add(m.Tamanho)
			continue
		}
		if err := g.baixarUm(ctx, m); err != nil {
			return err
		}
	}
	return nil
}

// baixarUm grava num temporário 0600, confere e só então dá o nome final;
// num erro, o temporário sai.
func (g *Gerente) baixarUm(ctx context.Context, m Modelo) (err error) {
	tmp, err := temporario(g.dir)
	if err != nil {
		return err
	}
	defer func() {
		tmp.Close()
		os.Remove(tmp.Name())
	}()
	if err := g.copiar(ctx, m, tmp); err != nil {
		return err
	}
	final := tmp
	if m.Extrair != "" {
		ext, err := temporario(g.dir)
		if err != nil {
			return err
		}
		defer func() {
			ext.Close()
			os.Remove(ext.Name())
		}()
		if err := extrair(tmp, m, ext); err != nil {
			return err
		}
		final = ext
	}
	if err := final.Sync(); err != nil {
		return err
	}
	if err := final.Close(); err != nil {
		return err
	}
	return os.Rename(final.Name(), filepath.Join(g.dir, m.Nome))
}

func temporario(dir string) (*os.File, error) {
	tmp, err := os.CreateTemp(dir, ".baixando-*")
	if err != nil {
		return nil, fmt.Errorf("Não consegui gravar o modelo: %w", err)
	}
	if err := tmp.Chmod(0o600); err != nil {
		tmp.Close()
		os.Remove(tmp.Name())
		return nil, err
	}
	return tmp, nil
}

// copiar baixa para o arquivo conferindo o tamanho e o SHA-256 no caminho.
func (g *Gerente) copiar(ctx context.Context, m Modelo, destino io.Writer) error {
	if !ehHTTPS(m.URL) {
		return errors.New("A fonte do modelo precisa ser https.")
	}
	// Sem nada chegando por um tempo, desiste (a conexão pode ficar pendurada).
	ctx, cancelar := context.WithCancel(ctx)
	defer cancelar()
	vigia := time.AfterFunc(semResposta, cancelar)
	defer vigia.Stop()

	pedido, err := http.NewRequestWithContext(ctx, http.MethodGet, m.URL, nil)
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
	if resposta.ContentLength >= 0 && resposta.ContentLength != m.Tamanho {
		return errors.New("A fonte mandou um arquivo de tamanho diferente do esperado; não baixei.")
	}
	hash := sha256.New()
	progresso := &contador{g: g, vigia: vigia}
	n, err := io.Copy(io.MultiWriter(destino, hash, progresso), io.LimitReader(resposta.Body, m.Tamanho+1))
	if err != nil {
		if ctx.Err() != nil {
			return ErrCancelado
		}
		return fmt.Errorf("O download parou no meio. Tente de novo. (%v)", err)
	}
	if n != m.Tamanho {
		return errors.New("O arquivo baixado tem tamanho diferente do esperado; apaguei.")
	}
	if !confere(hash, m.SHA256) {
		return errors.New("O arquivo baixado não confere com o esperado (SHA-256); apaguei.")
	}
	return nil
}

func confere(h hash.Hash, esperado string) bool { return hex.EncodeToString(h.Sum(nil)) == esperado }

// extrair tira do .tgz (já conferido) só o membro pedido, sem seguir links
// nem confiar no tamanho que o pacote diz, e confere o tamanho e o SHA-256
// dele.
func extrair(pacote *os.File, m Modelo, destino io.Writer) error {
	if _, err := pacote.Seek(0, io.SeekStart); err != nil {
		return err
	}
	gz, err := gzip.NewReader(pacote)
	if err != nil {
		return errors.New("O pacote baixado não abriu; apaguei.")
	}
	defer gz.Close()
	tr := tar.NewReader(gz)
	for {
		h, err := tr.Next()
		if err == io.EOF {
			return errors.New("O pacote baixado não tem o arquivo esperado; apaguei.")
		}
		if err != nil {
			return errors.New("O pacote baixado não abriu; apaguei.")
		}
		if h.Name != m.Extrair {
			continue
		}
		if h.Typeflag != tar.TypeReg {
			return errors.New("O arquivo esperado no pacote não é um arquivo comum; apaguei.")
		}
		hash := sha256.New()
		n, err := io.Copy(io.MultiWriter(destino, hash), io.LimitReader(tr, m.TamanhoFinal+1))
		if err != nil {
			return errors.New("O pacote baixado não abriu; apaguei.")
		}
		if n != m.TamanhoFinal || !confere(hash, m.SHA256Final) {
			return errors.New("O arquivo dentro do pacote não confere com o esperado (SHA-256); apaguei.")
		}
		return nil
	}
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
	if passo := c.g.total() / 100; passo == 0 || total-c.ultimo >= passo {
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
