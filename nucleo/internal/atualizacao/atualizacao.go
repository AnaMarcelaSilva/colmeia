package atualizacao

import (
	"archive/tar"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

// FontePadrao: os arquivos da última release do GitHub (o GitHub redireciona
// para a release mais nova).
const FontePadrao = "https://github.com/AnaMarcelaSilva/colmeia/releases/latest/download/"

const paginaReleases = "https://github.com/AnaMarcelaSilva/colmeia/releases/tag/v"

// Uma procura serve por este tempo: abrir a tela de novo não volta à rede.
const validade = time.Hour

// Sem receber nada por este tempo, o download desiste.
const semResposta = 60 * time.Second

// Maior pacote aceito e maior programa dentro dele.
const (
	limitePacote   = 400 << 20
	limitePrograma = 200 << 20
)

// Os programas que a atualização troca; a voz é opcional (o pacote pode não ter).
var programas = []struct {
	nome        string
	obrigatorio bool
}{{"colmeia", true}, {"colmeia-nucleo", true}, {"colmeia-abelha", true}, {"colmeia-voz", false}}

// Estados.
const (
	Atualizada  = "atualizada"
	Disponivel  = "disponivel"
	Baixando    = "baixando"
	Pronta      = "pronta"
	Reiniciando = "reiniciando"
	Falhou      = "erro"
)

type Estado struct {
	Estado string `json:"estado"`
	Atual  string `json:"atual"`
	// A versão nova e o que mudou nela, quando há uma.
	Versao string `json:"versao,omitempty"`
	Notas  string `json:"notas,omitempty"`
	// Instalavel: esta instalação troca os programas sozinha. Sem isso, a
	// tela leva à página da release.
	Instalavel bool   `json:"instalavel"`
	Pagina     string `json:"pagina,omitempty"`
	Baixados   int64  `json:"baixados"`
	Tamanho    int64  `json:"tamanho"`
	Erro       string `json:"erro,omitempty"`
	// Procurar ao abrir a Colmeia (as opções, veja Opcoes).
	Procurar bool `json:"procurar"`
	// Aviso diz como foi a última troca, uma vez.
	Aviso string `json:"aviso,omitempty"`
}

type Opcoes struct {
	Procurar bool `json:"procurar"`
}

type Gerente struct {
	Atual string
	// Fonte é a base das URLs (termina em /); Chave, a pública que confere.
	Fonte    string
	Chave    string
	Cliente  *http.Client
	Dir      string // <dados>/atualizacao
	DirBin   string // onde estão os programas
	DirDados string
	// CopiarBanco grava uma cópia consistente do banco em destino.
	CopiarBanco func(destino string) error
	// AoMudar recebe cada mudança de estado (e o progresso, por ponto percentual).
	AoMudar func(Estado)

	mu        sync.Mutex
	estado    Estado
	manifesto *Manifesto
	procurado time.Time
	ocupado   bool
	cancelar  context.CancelFunc
}

// Novo prepara o gerente; o resultado da última troca é lido em VerAviso
// (a troca grava depois que o núcleo novo já abriu).
func Novo(atual, dirDados, dirBin string) *Gerente {
	// COLMEIA_ATUALIZACAO_FONTE troca a fonte (para testar com uma release
	// servida no próprio computador); a assinatura é conferida igual.
	fonte := FontePadrao
	if f := os.Getenv("COLMEIA_ATUALIZACAO_FONTE"); f != "" {
		fonte = f
	}
	g := &Gerente{Atual: atual, Fonte: fonte, Chave: ChavePublica, Cliente: clientePadrao(),
		Dir: filepath.Join(dirDados, "atualizacao"), DirBin: dirBin, DirDados: dirDados}
	g.estado = Estado{Estado: Atualizada, Atual: atual}
	return g
}

func clientePadrao() *http.Client {
	t := http.DefaultTransport.(*http.Transport).Clone()
	t.TLSHandshakeTimeout = 20 * time.Second
	t.ResponseHeaderTimeout = 30 * time.Second
	return &http.Client{Transport: t, CheckRedirect: func(r *http.Request, via []*http.Request) error {
		if len(via) >= 5 {
			return errors.New("redirecionamentos demais")
		}
		return conferirURL(r.URL)
	}}
}

// conferirURL: só https (o GitHub redireciona para a CDN dele). O http vale
// só no próprio computador, para os testes; a assinatura é conferida igual.
func conferirURL(u *url.URL) error {
	if u.Scheme == "https" {
		return nil
	}
	if u.Scheme == "http" {
		if ip := net.ParseIP(u.Hostname()); ip != nil && ip.IsLoopback() {
			return nil
		}
	}
	return errors.New("a fonte da atualização precisa ser https")
}

// Estado devolve o estado atual, com as opções lidas do disco.
func (g *Gerente) Estado() Estado {
	g.mu.Lock()
	defer g.mu.Unlock()
	e := g.estado
	e.Procurar = g.opcoes().Procurar
	return e
}

func (g *Gerente) mudar(f func(*Estado)) {
	g.mu.Lock()
	f(&g.estado)
	e := g.estado
	e.Procurar = g.opcoes().Procurar
	g.mu.Unlock()
	if g.AoMudar != nil {
		g.AoMudar(e)
	}
}

// Opcoes: sem o arquivo, procura (é um pedido pequeno ao abrir).
func (g *Gerente) opcoes() Opcoes {
	o := Opcoes{Procurar: true}
	if bruto, err := os.ReadFile(filepath.Join(g.Dir, "opcoes.json")); err == nil {
		json.Unmarshal(bruto, &o)
	}
	return o
}

func (g *Gerente) GravarOpcoes(o Opcoes) error {
	if err := os.MkdirAll(g.Dir, 0o700); err != nil {
		return err
	}
	bruto, _ := json.Marshal(o)
	if err := gravar(filepath.Join(g.Dir, "opcoes.json"), bruto, 0o600); err != nil {
		return err
	}
	g.mudar(func(*Estado) {})
	return nil
}

// Procurar olha a release mais nova, uma vez por hora no máximo. Falta de
// internet não é erro para a tela: ela só não mostra nada.
func (g *Gerente) Procurar(ctx context.Context) {
	g.mu.Lock()
	recente := !g.procurado.IsZero() && time.Since(g.procurado) < validade
	if g.ocupado || recente || !g.opcoes().Procurar {
		g.mu.Unlock()
		return
	}
	g.ocupado = true
	g.mu.Unlock()
	defer func() {
		g.mu.Lock()
		g.ocupado = false
		g.mu.Unlock()
	}()

	m, err := g.lerManifesto(ctx)
	g.mu.Lock()
	g.procurado = time.Now()
	g.mu.Unlock()
	if err != nil {
		log.Printf("atualização: %v", err)
		return
	}
	// A versão que não abriu aqui não é oferecida de novo; a seguinte, sim.
	if !MaisNova(m.Versao, g.Atual) || m.Versao == g.versaoQueFalhou() {
		return
	}
	_, instalavel := g.pacote(m)
	instalavel = instalavel && g.podeTrocar()
	g.mu.Lock()
	g.manifesto = &m
	g.mu.Unlock()
	g.mudar(func(e *Estado) {
		*e = Estado{Estado: Disponivel, Atual: g.Atual, Versao: m.Versao, Notas: m.Notas, Instalavel: instalavel, Pagina: paginaReleases + m.Versao, Aviso: e.Aviso}
	})
}

// VerAviso devolve, uma vez, como foi a última troca.
func (g *Gerente) VerAviso() string {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.lerResultado()
}

func (g *Gerente) versaoQueFalhou() string {
	bruto, _ := os.ReadFile(filepath.Join(g.Dir, "falhou"))
	return strings.TrimSpace(string(bruto))
}

func (g *Gerente) lerManifesto(ctx context.Context) (Manifesto, error) {
	bruto, err := g.baixarPequeno(ctx, NomeManifesto, limiteManifesto)
	if err != nil {
		return Manifesto{}, err
	}
	sig, err := g.baixarPequeno(ctx, NomeAssinatura, 1<<10)
	if err != nil {
		return Manifesto{}, err
	}
	return Verificar(g.Chave, bruto, string(sig))
}

func (g *Gerente) baixarPequeno(ctx context.Context, nome string, limite int64) ([]byte, error) {
	ctx, cancelar := context.WithTimeout(ctx, 30*time.Second)
	defer cancelar()
	resposta, err := g.pedir(ctx, nome)
	if err != nil {
		return nil, err
	}
	defer resposta.Body.Close()
	bruto, err := io.ReadAll(io.LimitReader(resposta.Body, limite+1))
	if err != nil {
		return nil, err
	}
	if int64(len(bruto)) > limite {
		return nil, fmt.Errorf("%s grande demais", nome)
	}
	return bruto, nil
}

func (g *Gerente) pedir(ctx context.Context, nome string) (*http.Response, error) {
	u, err := url.Parse(g.Fonte + nome)
	if err != nil {
		return nil, err
	}
	if err := conferirURL(u); err != nil {
		return nil, err
	}
	pedido, err := http.NewRequestWithContext(ctx, http.MethodGet, u.String(), nil)
	if err != nil {
		return nil, err
	}
	resposta, err := g.Cliente.Do(pedido)
	if err != nil {
		return nil, err
	}
	if resposta.StatusCode != http.StatusOK {
		resposta.Body.Close()
		return nil, fmt.Errorf("%s: a fonte respondeu %s", nome, resposta.Status)
	}
	return resposta, nil
}

// pacote é o arquivo da release para este sistema.
func (g *Gerente) pacote(m Manifesto) (string, bool) {
	if runtime.GOOS != "linux" || runtime.GOARCH != "amd64" {
		return "", false
	}
	nome := "colmeia-linux-x86_64.tar.gz"
	_, ok := m.Arquivos[nome]
	return nome, ok
}

// podeTrocar: os programas estão juntos numa pasta onde dá para escrever
// (a instalação do instalar.sh). Rodando de dentro do repositório, não.
func (g *Gerente) podeTrocar() bool {
	if g.DirBin == "" {
		return false
	}
	for _, p := range programas {
		if p.obrigatorio {
			if info, err := os.Stat(filepath.Join(g.DirBin, p.nome)); err != nil || !info.Mode().IsRegular() {
				return false
			}
		}
	}
	teste, err := os.CreateTemp(g.DirBin, ".colmeia-teste-*")
	if err != nil {
		return false
	}
	teste.Close()
	os.Remove(teste.Name())
	return true
}

var ErrOcupado = errors.New("A atualização já está em andamento.")

// Cancelar interrompe o download.
func (g *Gerente) Cancelar() {
	g.mu.Lock()
	defer g.mu.Unlock()
	if g.cancelar != nil {
		g.cancelar()
	}
}

// Preparar baixa o pacote, confere, tira os programas para uma pasta, testa
// o núcleo novo e guarda uma cópia dos programas e do banco de agora. Não
// troca nada: isso fica para Aplicar.
func (g *Gerente) Preparar(ctx context.Context) error {
	g.mu.Lock()
	if g.ocupado || g.manifesto == nil || (g.estado.Estado != Disponivel && g.estado.Estado != Falhou) {
		pronta := g.estado.Estado == Pronta
		g.mu.Unlock()
		if pronta {
			return nil
		}
		return ErrOcupado
	}
	m := *g.manifesto
	ctx, cancelar := context.WithCancel(ctx)
	g.ocupado, g.cancelar = true, cancelar
	g.mu.Unlock()
	defer func() {
		cancelar()
		g.mu.Lock()
		g.ocupado, g.cancelar = false, nil
		g.mu.Unlock()
	}()

	err := g.preparar(ctx, m)
	if err != nil {
		if ctx.Err() != nil {
			err = errors.New("Download cancelado.")
		}
		g.mudar(func(e *Estado) { e.Estado, e.Erro = Falhou, err.Error() })
		return err
	}
	g.mudar(func(e *Estado) { e.Estado, e.Erro = Pronta, "" })
	return nil
}

func (g *Gerente) preparar(ctx context.Context, m Manifesto) error {
	nome, ok := g.pacote(m)
	if !ok || !g.podeTrocar() {
		return errors.New("Esta instalação não troca os programas sozinha. Baixe pela página da versão.")
	}
	a := m.Arquivos[nome]
	if a.Tamanho > limitePacote {
		return errors.New("O pacote da versão nova é grande demais; não baixei.")
	}
	g.mudar(func(e *Estado) { e.Estado, e.Erro, e.Baixados, e.Tamanho = Baixando, "", 0, a.Tamanho })
	if err := os.MkdirAll(g.Dir, 0o700); err != nil {
		return err
	}
	baixado := filepath.Join(g.Dir, "baixando.tar.gz")
	defer os.Remove(baixado)
	if err := g.baixarPacote(ctx, nome, a, baixado); err != nil {
		return err
	}
	nova := filepath.Join(g.Dir, "nova")
	os.RemoveAll(nova)
	if err := extrair(baixado, nova); err != nil {
		os.RemoveAll(nova)
		return err
	}
	if err := testarNucleo(ctx, filepath.Join(nova, "colmeia-nucleo"), m.Versao); err != nil {
		os.RemoveAll(nova)
		return err
	}
	if err := g.guardarAnterior(); err != nil {
		os.RemoveAll(nova)
		return fmt.Errorf("Não consegui guardar a cópia da versão de agora: %v", err)
	}
	p := pendente{De: g.Atual, Para: m.Versao, DirBin: g.DirBin, DirDados: g.DirDados}
	bruto, _ := json.Marshal(p)
	return gravar(filepath.Join(g.Dir, "pendente.json"), bruto, 0o600)
}

func (g *Gerente) baixarPacote(ctx context.Context, nome string, a Arquivo, destino string) error {
	ctx, cancelar := context.WithCancel(ctx)
	defer cancelar()
	vigia := time.AfterFunc(semResposta, cancelar)
	defer vigia.Stop()
	resposta, err := g.pedir(ctx, nome)
	if err != nil {
		return fmt.Errorf("Não consegui baixar a versão nova. Confira a internet e tente de novo. (%v)", err)
	}
	defer resposta.Body.Close()
	if resposta.ContentLength >= 0 && resposta.ContentLength != a.Tamanho {
		return errors.New("A fonte mandou um pacote de tamanho diferente do esperado; não baixei.")
	}
	arquivo, err := os.OpenFile(destino, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	defer arquivo.Close()
	hash := sha256.New()
	progresso := &contador{g: g, vigia: vigia, total: a.Tamanho}
	n, err := io.Copy(io.MultiWriter(arquivo, hash, progresso), io.LimitReader(resposta.Body, a.Tamanho+1))
	if err != nil {
		return fmt.Errorf("O download parou no meio. Tente de novo. (%v)", err)
	}
	if n != a.Tamanho || hex.EncodeToString(hash.Sum(nil)) != a.SHA256 {
		return errors.New("O pacote baixado não confere com o assinado; apaguei.")
	}
	return arquivo.Close()
}

// contador soma o que chega, segura o vigia e avisa a cada ponto percentual.
type contador struct {
	g      *Gerente
	vigia  *time.Timer
	total  int64
	soma   int64
	ultimo int64
}

func (c *contador) Write(p []byte) (int, error) {
	c.vigia.Reset(semResposta)
	c.soma += int64(len(p))
	if ponto := c.soma * 100 / c.total; ponto != c.ultimo {
		c.ultimo = ponto
		soma := c.soma
		c.g.mudar(func(e *Estado) { e.Baixados = soma })
	}
	return len(p), nil
}

// extrair tira do pacote (já conferido) só os programas conhecidos, como
// arquivos comuns, sem seguir links nem aceitar caminhos.
func extrair(pacote, destino string) error {
	arquivo, err := os.Open(pacote)
	if err != nil {
		return err
	}
	defer arquivo.Close()
	gz, err := gzip.NewReader(arquivo)
	if err != nil {
		return errors.New("O pacote baixado não abriu; apaguei.")
	}
	defer gz.Close()
	if err := os.MkdirAll(destino, 0o700); err != nil {
		return err
	}
	conhecidos := map[string]bool{}
	for _, p := range programas {
		conhecidos[p.nome] = true
	}
	tirados := map[string]bool{}
	tr := tar.NewReader(gz)
	for {
		h, err := tr.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return errors.New("O pacote baixado não abriu; apaguei.")
		}
		nome := strings.TrimPrefix(h.Name, "./")
		if !conhecidos[nome] {
			continue
		}
		if h.Typeflag != tar.TypeReg || tirados[nome] {
			return errors.New("O pacote baixado tem um programa fora do formato; apaguei.")
		}
		tirados[nome] = true
		saida, err := os.OpenFile(filepath.Join(destino, nome), os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o700)
		if err != nil {
			return err
		}
		n, err := io.Copy(saida, io.LimitReader(tr, limitePrograma+1))
		saida.Close()
		if err != nil || n > limitePrograma {
			return errors.New("O pacote baixado tem um programa fora do formato; apaguei.")
		}
	}
	for _, p := range programas {
		if p.obrigatorio && !tirados[p.nome] {
			return fmt.Errorf("O pacote baixado não tem o programa %s; apaguei.", p.nome)
		}
	}
	return nil
}

// testarNucleo roda o núcleo novo com --versao: ele precisa abrir e dizer
// a versão do manifesto.
func testarNucleo(ctx context.Context, caminho, versao string) error {
	ctx, cancelar := context.WithTimeout(ctx, 15*time.Second)
	defer cancelar()
	saida, err := exec.CommandContext(ctx, caminho, "--versao").Output()
	if err != nil || strings.TrimSpace(string(saida)) != "colmeia-nucleo "+versao {
		return errors.New("O núcleo da versão nova não abriu aqui; não instalei.")
	}
	return nil
}

// guardarAnterior copia os programas de agora e o banco para <Dir>/anterior.
func (g *Gerente) guardarAnterior() error {
	anterior := filepath.Join(g.Dir, "anterior")
	os.RemoveAll(anterior)
	if err := os.MkdirAll(anterior, 0o700); err != nil {
		return err
	}
	for _, p := range programas {
		origem := filepath.Join(g.DirBin, p.nome)
		if _, err := os.Stat(origem); err != nil {
			if p.obrigatorio {
				return err
			}
			continue
		}
		if err := copiar(origem, filepath.Join(anterior, p.nome), 0o700); err != nil {
			return err
		}
	}
	if g.CopiarBanco != nil {
		return g.CopiarBanco(filepath.Join(anterior, "colmeia.db"))
	}
	return nil
}

// pendente é o que a troca precisa saber; fica em <Dir>/pendente.json.
type pendente struct {
	De       string `json:"de"`
	Para     string `json:"para"`
	DirBin   string `json:"dir_bin"`
	DirDados string `json:"dir_dados"`
}

// resultado é o que a troca deixa para a tela ver depois.
type resultado struct {
	OK   bool   `json:"ok"`
	De   string `json:"de"`
	Para string `json:"para"`
	Erro string `json:"erro,omitempty"`
}

func (g *Gerente) lerResultado() string {
	caminho := filepath.Join(g.Dir, "resultado.json")
	bruto, err := os.ReadFile(caminho)
	if err != nil {
		return ""
	}
	os.Remove(caminho)
	var r resultado
	if json.Unmarshal(bruto, &r) != nil {
		return ""
	}
	if r.OK {
		return "A Colmeia foi atualizada para a versão " + r.Para + "."
	}
	return "A versão " + r.Para + " não abriu e a Colmeia voltou para a " + r.De + ". Nada se perdeu."
}

// Aplicar abre a troca, solta deste núcleo, e devolve; quem chama encerra
// o núcleo em seguida. A troca roda com a cópia do núcleo de agora (o
// programa que já está aqui, não o baixado) e espera este processo sair.
func (g *Gerente) Aplicar(abrirTela bool) error {
	g.mu.Lock()
	pronta := g.estado.Estado == Pronta
	g.mu.Unlock()
	if !pronta {
		return errors.New("A versão nova ainda não está pronta.")
	}
	args := []string{"--trocar", g.Dir, "--esperar", fmt.Sprint(os.Getpid())}
	if abrirTela {
		args = append(args, "--abrir-tela")
	}
	if err := iniciarSolto(filepath.Join(g.Dir, "anterior", "colmeia-nucleo"), args...); err != nil {
		return fmt.Errorf("Não consegui começar a troca: %v", err)
	}
	g.mudar(func(e *Estado) { e.Estado = Reiniciando })
	return nil
}

func copiar(origem, destino string, modo os.FileMode) error {
	entrada, err := os.Open(origem)
	if err != nil {
		return err
	}
	defer entrada.Close()
	saida, err := os.OpenFile(destino, os.O_CREATE|os.O_TRUNC|os.O_WRONLY, modo)
	if err != nil {
		return err
	}
	if _, err := io.Copy(saida, entrada); err != nil {
		saida.Close()
		return err
	}
	return saida.Close()
}

// gravar escreve num temporário ao lado e troca de nome.
func gravar(caminho string, conteudo []byte, modo os.FileMode) error {
	tmp := caminho + ".tmp"
	if err := os.WriteFile(tmp, conteudo, modo); err != nil {
		return err
	}
	return os.Rename(tmp, caminho)
}
