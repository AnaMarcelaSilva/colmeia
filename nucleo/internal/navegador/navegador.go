// Package navegador abre um Chrome ou Chromium controlado pela Colmeia, ao
// lado da janela, para o usuário e o agente da tarefa verem e capturarem a tela
// que o agente criou. O controle é só pelo pipe (--remote-debugging-pipe):
// nenhuma porta é aberta. Cada perfil da Colmeia tem um processo do
// navegador, com uma pasta de perfil própria (0700, nunca a do usuário), e cada
// tarefa tem a sua janela nele.
package navegador

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/arquivos"
)

// Candidatos, na ordem, procurados no PATH. A lista é fixa e roda sem shell.
var Candidatos = []string{"chromium", "chromium-browser", "google-chrome-stable", "google-chrome"}

var (
	ErrSemNavegador = errors.New("Nenhum Chrome ou Chromium instalado. Instale o Chromium ou aponte COLMEIA_NAVEGADOR.")
	ErrURL          = errors.New("Só endereços http, https ou arquivos desta pasta")
	ErrFechado      = errors.New("o navegador desta tarefa não está aberto")
	ErrSensivel     = errors.New("Este arquivo pode ter senhas ou chaves; abra pela gaveta de Arquivos")
	// ErrSaiuDaPasta: a página foi (sozinha ou por um link) para um arquivo
	// que o navegador não deveria mostrar; a captura é recusada.
	ErrSaiuDaPasta = errors.New("A página do navegador saiu da pasta da tarefa; a captura foi recusada")
)

// Encontrar acha o executável: COLMEIA_NAVEGADOR (um caminho ou um nome no
// PATH) ou o primeiro dos candidatos.
func Encontrar() (caminho, nome string, err error) {
	if v := os.Getenv("COLMEIA_NAVEGADOR"); v != "" {
		if c, err := exec.LookPath(v); err == nil {
			return c, filepath.Base(c), nil
		}
		return "", "", ErrSemNavegador
	}
	for _, n := range Candidatos {
		if c, err := exec.LookPath(n); err == nil {
			return c, n, nil
		}
	}
	// No Windows o Chrome e o Edge não ficam no PATH: os lugares de instalação.
	for _, c := range instalados() {
		if info, err := os.Stat(c); err == nil && !info.IsDir() {
			return c, filepath.Base(c), nil
		}
	}
	return "", "", ErrSemNavegador
}

// Geometria da janela, em pixels da tela.
type Geometria struct {
	X       int `json:"x"`
	Y       int `json:"y"`
	Largura int `json:"largura"`
	Altura  int `json:"altura"`
}

// Valida confere que a geometria é plausível.
func (g Geometria) Valida() bool {
	return g.Largura > 0 && g.Altura > 0 && g.Largura <= 8192 && g.Altura <= 8192 && g.X > -8192 && g.Y > -8192 && g.X < 16384 && g.Y < 16384
}

// Menor janela que vale a pena abrir ou guardar. Uma geometria menor veio de
// uma medida errada da tela (já veio 500x88 no canto, por cima do logo): é
// ignorada, para não ficar gravada para as próximas aberturas.
const (
	LarguraMinima = 360
	AlturaMinima  = 300
)

// Usavel: válida e de um tamanho em que dá para ver uma página.
func (g Geometria) Usavel() bool {
	return g.Valida() && g.Largura >= LarguraMinima && g.Altura >= AlturaMinima
}

// URL conferida: o endereço que vai para o navegador e a descrição que pode
// ir para os eventos (sem a query, que pode ter tokens).
type URL struct {
	Endereco  string
	Descricao string
}

// ValidarURL aceita http e https (com host, sem usuário e senha) e file://
// de dentro da pasta da tarefa. O resto é recusado.
func ValidarURL(bruto, pasta string) (URL, error) {
	bruto = strings.TrimSpace(bruto)
	if bruto == "" || len(bruto) > 4096 || strings.ContainsFunc(bruto, func(r rune) bool { return r < ' ' || r == 0x7f }) {
		return URL{}, ErrURL
	}
	u, err := url.Parse(bruto)
	if err != nil {
		return URL{}, ErrURL
	}
	switch strings.ToLower(u.Scheme) {
	case "http", "https":
		if u.Host == "" || u.User != nil || u.Opaque != "" {
			return URL{}, ErrURL
		}
		u.Scheme = strings.ToLower(u.Scheme)
		descricao := u.Host + u.EscapedPath()
		return URL{Endereco: u.String(), Descricao: descricao}, nil
	case "file":
		if pasta == "" || (u.Host != "" && u.Host != "localhost") || u.Opaque != "" || u.RawQuery != "" {
			return URL{}, ErrURL
		}
		caminho := caminhoDoURL(u.Path)
		rel, err := arquivos.Relativo(pasta, caminho)
		if err != nil || !filepath.IsAbs(caminho) {
			return URL{}, ErrURL
		}
		raiz, err := os.OpenRoot(pasta)
		if err != nil {
			return URL{}, ErrURL
		}
		defer raiz.Close()
		// Stat pelo os.Root segue links só dentro da pasta.
		if _, err := raiz.Stat(rel); err != nil {
			return URL{}, ErrURL
		}
		if arquivos.Sensivel(filepath.Base(rel)) {
			return URL{}, ErrSensivel
		}
		final := url.URL{Scheme: "file", Path: caminhoNoURL(filepath.Join(pasta, rel)), Fragment: u.Fragment}
		return URL{Endereco: final.String(), Descricao: "arquivo " + rel}, nil
	}
	return URL{}, ErrURL
}

// caminhoDoURL é o caminho de um file:// no sistema: no Windows o URL traz
// "/C:/pasta/arquivo", e o caminho é "C:\pasta\arquivo".
func caminhoDoURL(p string) string {
	if runtime.GOOS != "windows" {
		return p
	}
	if len(p) >= 3 && p[0] == '/' && p[2] == ':' {
		p = p[1:]
	}
	return filepath.FromSlash(p)
}

// caminhoNoURL é o inverso: o caminho do sistema no formato do file://.
func caminhoNoURL(c string) string {
	if runtime.GOOS != "windows" {
		return c
	}
	return "/" + filepath.ToSlash(c)
}

// ArquivoDaPasta confere um file:// que a página pede ao navegador
// (navegação, redirecionamento, iframe, imagem, script): só de dentro da
// pasta, com links seguidos só lá dentro, fora do .git e sem os arquivos
// sensíveis. Diferente de ValidarURL, aceita query e node_modules (uma página
// da tarefa carrega os próprios scripts).
func ArquivoDaPasta(endereco, pasta string) error {
	u, err := url.Parse(endereco)
	if err != nil || !strings.EqualFold(u.Scheme, "file") || pasta == "" || (u.Host != "" && u.Host != "localhost") || u.Opaque != "" {
		return ErrURL
	}
	caminho := caminhoDoURL(u.Path)
	if strings.ContainsRune(caminho, 0) || !filepath.IsAbs(caminho) {
		return ErrURL
	}
	rel, err := filepath.Rel(filepath.Clean(pasta), filepath.Clean(caminho))
	if err != nil || rel == ".." || strings.HasPrefix(rel, ".."+string(filepath.Separator)) || filepath.IsAbs(rel) {
		return ErrURL
	}
	for parte := range strings.SplitSeq(rel, string(filepath.Separator)) {
		if parte == ".git" {
			return ErrURL
		}
	}
	if arquivos.Sensivel(filepath.Base(rel)) {
		return ErrSensivel
	}
	raiz, err := os.OpenRoot(pasta)
	if err != nil {
		return ErrURL
	}
	defer raiz.Close()
	if _, err := raiz.Stat(rel); err != nil {
		return ErrURL
	}
	return nil
}

// descrever diz o que a janela mostra agora, a partir do endereço atual da
// página. Recusa (ErrSaiuDaPasta) o que o navegador não poderia ter aberto.
func descrever(endereco, pasta string) (string, error) {
	u, err := url.Parse(endereco)
	if err != nil {
		return "", ErrSaiuDaPasta
	}
	switch strings.ToLower(u.Scheme) {
	case "http", "https":
		if u.Host == "" || u.User != nil {
			return "", ErrSaiuDaPasta
		}
		return u.Host + u.EscapedPath(), nil
	case "file":
		if err := ArquivoDaPasta(endereco, pasta); err != nil {
			return "", ErrSaiuDaPasta
		}
		rel, _ := filepath.Rel(filepath.Clean(pasta), filepath.Clean(caminhoDoURL(u.Path)))
		return "arquivo " + rel, nil
	case "about":
		if endereco == "about:blank" {
			return "", nil
		}
	case "chrome-error":
		// A página de erro do próprio navegador (endereço fora do ar, ou um
		// arquivo recusado pela Colmeia): não mostra nada da máquina.
		return "página de erro", nil
	}
	return "", ErrSaiuDaPasta
}

// Mudanca avisa que a janela de uma tarefa fechou (o usuário fechou ou o
// navegador saiu).
type Mudanca struct {
	Perfil, Tarefa int64
	Tipo           string // "fechado"
}

// Gerente guarda um navegador por perfil.
type Gerente struct {
	// Dir é onde ficam as pastas de perfil: <Dir>/<perfil>.
	Dir string
	// Headless: sem janela (testes).
	Headless bool
	// AoMudar recebe o fechamento das janelas. Não pode bloquear.
	AoMudar func(Mudanca)
	// iniciar abre o processo (os testes trocam por um Chrome falso).
	iniciar func(perfil int64, g Geometria) (*processo, error)

	mu          sync.Mutex
	perfis      map[int64]*processo
	geometri    map[int64]Geometria
	apresentado map[int64]bool
}

// ForaDaTela é onde a janela aberta pelo agente fica durante uma
// apresentação: existe e desenha (a captura funciona), mas não cobre o
// slide compartilhado. Quando o usuário pede o navegador, ele vem para o lado.
const ForaDaTela = 10000

// Apresentando avisa que a tela do perfil está (ou não) em apresentação.
func (g *Gerente) Apresentando(perfil int64, sim bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	if sim {
		g.apresentado[perfil] = true
	} else {
		delete(g.apresentado, perfil)
	}
}

func (g *Gerente) emApresentacao(perfil int64) bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.apresentado[perfil]
}

func NovoGerente(dir string) *Gerente {
	g := &Gerente{Dir: dir, perfis: map[int64]*processo{}, geometri: map[int64]Geometria{}, apresentado: map[int64]bool{}}
	g.iniciar = g.iniciarChrome
	return g
}

// processo é um navegador aberto para um perfil.
type processo struct {
	conn *conexao
	// matar encerra o navegador e o que ele abriu, à força.
	matar   func()
	perfil  int64
	mu      sync.Mutex
	janelas map[int64]*janela // por tarefa
	// preparando: janelas novas, antes de entrarem em janelas (a conferência
	// dos arquivos já vale para elas).
	preparando map[string]*janela
	// sobra: a primeira aba, quando o navegador abriu uma janela sozinho.
	sobra string
	fim   chan struct{}
}

type janela struct {
	alvo, sessao string
	descricao    string
	// pasta da tarefa: os file:// que a página pede são conferidos nela.
	pasta string
}

// Info é o que a tela e o agente sabem da janela de uma tarefa.
type Info struct {
	Aberto    bool   `json:"aberto"`
	Descricao string `json:"descricao,omitempty"`
}

// Lembrar guarda a última geometria pedida pela tela para o perfil: o agente
// abre a janela no mesmo lugar.
func (g *Gerente) Lembrar(perfil int64, geo Geometria) {
	if !geo.Usavel() {
		return
	}
	g.mu.Lock()
	g.geometri[perfil] = geo
	g.mu.Unlock()
}

func (g *Gerente) geometria(perfil int64) Geometria {
	g.mu.Lock()
	defer g.mu.Unlock()
	if geo, ok := g.geometri[perfil]; ok {
		return geo
	}
	// Sem nada da tela (ela avisa ao conectar): a metade direita de um
	// monitor comum, sem passar da altura dele.
	return Geometria{X: 960, Y: 0, Largura: 960, Altura: 900}
}

// Aberto diz se a tarefa tem janela aberta.
func (g *Gerente) Aberto(perfil, tarefa int64) Info {
	g.mu.Lock()
	p := g.perfis[perfil]
	g.mu.Unlock()
	if p == nil {
		return Info{}
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	if j, ok := p.janelas[tarefa]; ok {
		return Info{Aberto: true, Descricao: j.descricao}
	}
	return Info{}
}

// processoDo devolve o navegador do perfil, abrindo se preciso.
func (g *Gerente) processoDo(perfil int64) (*processo, bool, error) {
	g.mu.Lock()
	p := g.perfis[perfil]
	g.mu.Unlock()
	if p != nil {
		select {
		case <-p.fim:
		default:
			return p, false, nil
		}
	}
	p, err := g.iniciar(perfil, g.geometria(perfil))
	if err != nil {
		return nil, false, err
	}
	p.conn.mu.Lock()
	p.conn.aoEvento = func(sessao, metodo string, params json.RawMessage) { g.evento(p, sessao, metodo, params) }
	p.conn.mu.Unlock()
	g.mu.Lock()
	g.perfis[perfil] = p
	g.mu.Unlock()
	go func() {
		<-p.fim
		g.mu.Lock()
		if g.perfis[perfil] == p {
			delete(g.perfis, perfil)
		}
		g.mu.Unlock()
		p.mu.Lock()
		janelas := p.janelas
		p.janelas = map[int64]*janela{}
		p.mu.Unlock()
		for tarefa := range janelas {
			g.avisar(Mudanca{Perfil: perfil, Tarefa: tarefa, Tipo: "fechado"})
		}
	}()
	return p, true, nil
}

func (g *Gerente) avisar(m Mudanca) {
	if g.AoMudar != nil {
		g.AoMudar(m)
	}
}

// janelaDaSessao acha a janela (e a tarefa) de uma sessão.
func (p *processo) janelaDaSessao(sessao string) *janela {
	p.mu.Lock()
	defer p.mu.Unlock()
	for _, j := range p.janelas {
		if j.sessao == sessao {
			return j
		}
	}
	return p.preparando[sessao]
}

// evento trata os eventos do navegador: uma janela fechada pelo usuário some
// do mapa e vira aviso; cada arquivo que a página pede passa pela pasta da
// tarefa; a navegação da janela atualiza a descrição.
func (g *Gerente) evento(p *processo, sessao, metodo string, params json.RawMessage) {
	switch metodo {
	case "Fetch.requestPaused":
		// Responder pede uma chamada ao navegador: fora da leitura.
		go g.conferirPedido(p, sessao, params)
		return
	case "Page.frameNavigated":
		var d struct {
			Quadro struct {
				Pai string `json:"parentId"`
				URL string `json:"url"`
			} `json:"frame"`
		}
		if json.Unmarshal(params, &d) != nil || d.Quadro.Pai != "" {
			return
		}
		if j := p.janelaDaSessao(sessao); j != nil {
			p.mu.Lock()
			if descricao, err := descrever(d.Quadro.URL, j.pasta); err == nil {
				if descricao != "" {
					j.descricao = descricao
				}
			} else {
				j.descricao = "fora da pasta"
			}
			p.mu.Unlock()
		}
		return
	}
	if metodo != "Target.targetDestroyed" || sessao != "" {
		return
	}
	var d struct {
		Alvo string `json:"targetId"`
	}
	json.Unmarshal(params, &d)
	p.mu.Lock()
	var fechada int64
	for tarefa, j := range p.janelas {
		if j.alvo == d.Alvo {
			fechada = tarefa
			delete(p.janelas, tarefa)
		}
	}
	if p.sobra == d.Alvo {
		p.sobra = ""
	}
	p.mu.Unlock()
	if fechada != 0 {
		g.avisar(Mudanca{Perfil: p.perfil, Tarefa: fechada, Tipo: "fechado"})
	}
}

// conferirPedido libera ou recusa um file:// pedido pela página (as outras
// URLs não são interceptadas). Na dúvida (sessão desconhecida), recusa.
func (g *Gerente) conferirPedido(p *processo, sessao string, params json.RawMessage) {
	var d struct {
		ID     string `json:"requestId"`
		Pedido struct {
			URL string `json:"url"`
		} `json:"request"`
	}
	if json.Unmarshal(params, &d) != nil || d.ID == "" {
		return
	}
	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	j := p.janelaDaSessao(sessao)
	if j != nil && ArquivoDaPasta(d.Pedido.URL, j.pasta) == nil {
		p.conn.chamar(ctx, sessao, "Fetch.continueRequest", map[string]any{"requestId": d.ID}, nil)
		return
	}
	p.conn.chamar(ctx, sessao, "Fetch.failRequest", map[string]any{"requestId": d.ID, "errorReason": "AccessDenied"}, nil)
}

// Pedido de abrir a janela de uma tarefa.
type Pedido struct {
	Perfil, Tarefa int64
	// Pasta da tarefa: só os arquivos dela abrem no navegador.
	Pasta     string
	URL       URL // vazio: só traz para frente (ou abre em branco)
	Geometria *Geometria
	// Fundo: abre sem tomar o foco (o agente abrindo durante uma apresentação).
	Fundo bool
}

// Abrir abre a janela da tarefa (ou traz para frente) e vai para o endereço,
// esperando a página carregar (até 15 s). Devolve se a janela é nova.
func (g *Gerente) Abrir(ctx context.Context, pe Pedido) (nova bool, err error) {
	if pe.Geometria != nil && !pe.Geometria.Usavel() {
		pe.Geometria = nil
	}
	if pe.Geometria != nil {
		g.Lembrar(pe.Perfil, *pe.Geometria)
	}
	p, _, err := g.processoDo(pe.Perfil)
	if err != nil {
		return false, err
	}
	ctx, cancelar := context.WithTimeout(ctx, 20*time.Second)
	defer cancelar()
	p.mu.Lock()
	j := p.janelas[pe.Tarefa]
	if j != nil && pe.Pasta != "" {
		j.pasta = pe.Pasta
	}
	p.mu.Unlock()
	if j == nil {
		if j, err = g.novaJanela(ctx, p, pe); err != nil {
			return false, err
		}
		nova = true
	}
	if pe.URL.Endereco != "" {
		if err := g.navegar(ctx, p, j, pe.URL); err != nil {
			// Uma janela nova que não chegou ao endereço não fica: sem isso
			// ela ficaria na tela (por cima do aviso de erro) sem a Colmeia
			// contar como aberta. A que já existia fica, com a página de erro.
			if nova {
				g.descartar(p, pe.Tarefa, j)
			}
			return false, err
		}
	}
	if !nova && !pe.Fundo {
		// O usuário pediu: a janela vem para o lado da Colmeia (pode ter ficado
		// fora da tela durante uma apresentação) e para frente. Só depois
		// de navegar: se o endereço falhar, o aviso na Colmeia fica visível.
		if pe.Geometria != nil {
			g.posicionar(ctx, p, j.alvo, *pe.Geometria)
		}
		p.conn.chamar(ctx, "", "Target.activateTarget", map[string]any{"targetId": j.alvo}, nil)
	}
	return nova, nil
}

// ErrEndereco: o navegador não chegou ao endereço (domínio que não existe,
// servidor fora do ar). Motivo é o código do navegador (net::ERR_…).
type ErrEndereco struct{ Motivo string }

func (e ErrEndereco) Error() string { return "o endereço não abriu: " + e.Motivo }

// navegar leva a janela ao endereço e espera a página carregar (até 15 s).
func (g *Gerente) navegar(ctx context.Context, p *processo, j *janela, u URL) error {
	carregou, parar := p.conn.ouvir(j.sessao, "Page.loadEventFired")
	defer parar()
	var r struct {
		Erro string `json:"errorText"`
	}
	if err := p.conn.chamar(ctx, j.sessao, "Page.navigate", map[string]any{"url": u.Endereco}, &r); err != nil {
		return err
	}
	if r.Erro != "" {
		return ErrEndereco{Motivo: r.Erro}
	}
	p.mu.Lock()
	j.descricao = u.Descricao
	p.mu.Unlock()
	select {
	case <-carregou.ch:
	case <-ctx.Done():
	case <-time.After(15 * time.Second):
	}
	return nil
}

// descartar fecha a janela nova que não chegou a abrir para a tarefa (sem
// aviso: para a Colmeia ela nunca esteve aberta).
func (g *Gerente) descartar(p *processo, tarefa int64, j *janela) {
	p.mu.Lock()
	if p.janelas[tarefa] == j {
		delete(p.janelas, tarefa)
	}
	p.mu.Unlock()
	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	p.conn.chamar(ctx, "", "Target.closeTarget", map[string]any{"targetId": j.alvo}, nil)
}

func (g *Gerente) novaJanela(ctx context.Context, p *processo, pe Pedido) (*janela, error) {
	geo := g.geometria(pe.Perfil)
	var alvo struct {
		ID string `json:"targetId"`
	}
	p.mu.Lock()
	sobra := p.sobra
	p.sobra = ""
	p.mu.Unlock()
	if sobra != "" {
		alvo.ID = sobra
	} else {
		params := map[string]any{"url": "about:blank", "newWindow": true}
		if pe.Fundo {
			params["background"] = true
		}
		if err := p.conn.chamar(ctx, "", "Target.createTarget", params, &alvo); err != nil {
			return nil, err
		}
	}
	var sessao struct {
		ID string `json:"sessionId"`
	}
	if err := p.conn.chamar(ctx, "", "Target.attachToTarget", map[string]any{"targetId": alvo.ID, "flatten": true}, &sessao); err != nil {
		return nil, err
	}
	// Antes de qualquer navegação: todo file:// que a página pedir (o
	// endereço, um redirecionamento, um iframe, uma imagem) passa pela
	// conferência da pasta. Sem isso, uma página da pasta poderia ir sozinha
	// para um arquivo de fora e a captura levaria o conteúdo dele.
	j := &janela{alvo: alvo.ID, sessao: sessao.ID, pasta: pe.Pasta}
	p.mu.Lock()
	p.preparando[sessao.ID] = j
	p.mu.Unlock()
	defer func() {
		p.mu.Lock()
		delete(p.preparando, sessao.ID)
		p.mu.Unlock()
	}()
	if err := p.conn.chamar(ctx, sessao.ID, "Fetch.enable", map[string]any{"patterns": []any{map[string]any{"urlPattern": "file:*"}}}, nil); err != nil {
		return nil, err
	}
	if err := p.conn.chamar(ctx, sessao.ID, "Page.enable", nil, nil); err != nil {
		return nil, err
	}
	// Posição ao lado da Colmeia (no Wayland o compositor pode ignorar). O
	// agente abrindo durante uma apresentação: fora da tela, sem cobrir o slide.
	if pe.Fundo && g.emApresentacao(pe.Perfil) {
		geo.X, geo.Y = ForaDaTela, 0
	}
	g.posicionar(ctx, p, alvo.ID, geo)
	p.mu.Lock()
	p.janelas[pe.Tarefa] = j
	p.mu.Unlock()
	return j, nil
}

// posicionar move e redimensiona a janela do alvo.
func (g *Gerente) posicionar(ctx context.Context, p *processo, alvo string, geo Geometria) {
	var janelaID struct {
		ID int `json:"windowId"`
	}
	if p.conn.chamar(ctx, "", "Browser.getWindowForTarget", map[string]any{"targetId": alvo}, &janelaID) == nil {
		p.conn.chamar(ctx, "", "Browser.setWindowBounds", map[string]any{"windowId": janelaID.ID,
			"bounds": map[string]any{"left": geo.X, "top": geo.Y, "width": geo.Largura, "height": geo.Altura, "windowState": "normal"}}, nil)
	}
}

// Capturar tira um PNG do que a janela da tarefa mostra agora.
func (g *Gerente) Capturar(ctx context.Context, perfil, tarefa int64) ([]byte, string, error) {
	g.mu.Lock()
	p := g.perfis[perfil]
	g.mu.Unlock()
	if p == nil {
		return nil, "", ErrFechado
	}
	p.mu.Lock()
	j := p.janelas[tarefa]
	var pasta string
	if j != nil {
		pasta = j.pasta
	}
	p.mu.Unlock()
	if j == nil {
		return nil, "", ErrFechado
	}
	ctx, cancelar := context.WithTimeout(ctx, 20*time.Second)
	defer cancelar()
	// O que vale é o endereço de agora, não o que foi aberto: a página pode
	// ter ido para outro lugar sozinha.
	var info struct {
		Alvo struct {
			URL string `json:"url"`
		} `json:"targetInfo"`
	}
	if err := p.conn.chamar(ctx, "", "Target.getTargetInfo", map[string]any{"targetId": j.alvo}, &info); err != nil {
		return nil, "", err
	}
	descricao, err := descrever(info.Alvo.URL, pasta)
	if err != nil {
		return nil, "", err
	}
	p.mu.Lock()
	if descricao != "" {
		j.descricao = descricao
	}
	p.mu.Unlock()
	var r struct {
		Dados string `json:"data"`
	}
	if err := p.conn.chamar(ctx, j.sessao, "Page.captureScreenshot", map[string]any{"format": "png"}, &r); err != nil {
		return nil, "", err
	}
	png, err := base64.StdEncoding.DecodeString(r.Dados)
	if err != nil {
		return nil, "", errors.New("o navegador devolveu uma captura inválida")
	}
	return png, descricao, nil
}

// Fechar fecha a janela da tarefa.
func (g *Gerente) Fechar(ctx context.Context, perfil, tarefa int64) error {
	g.mu.Lock()
	p := g.perfis[perfil]
	g.mu.Unlock()
	if p == nil {
		return ErrFechado
	}
	p.mu.Lock()
	j := p.janelas[tarefa]
	delete(p.janelas, tarefa)
	p.mu.Unlock()
	if j == nil {
		return ErrFechado
	}
	ctx, cancelar := context.WithTimeout(ctx, 5*time.Second)
	defer cancelar()
	err := p.conn.chamar(ctx, "", "Target.closeTarget", map[string]any{"targetId": j.alvo}, nil)
	g.avisar(Mudanca{Perfil: perfil, Tarefa: tarefa, Tipo: "fechado"})
	return err
}

// FecharTodos fecha os navegadores ao encerrar o núcleo: pede com
// Browser.close e, depois de 3 s, encerra o grupo de processos à força.
func (g *Gerente) FecharTodos() {
	g.mu.Lock()
	perfis := g.perfis
	g.perfis = map[int64]*processo{}
	g.mu.Unlock()
	var espera sync.WaitGroup
	for _, p := range perfis {
		espera.Go(func() { p.encerrar() })
	}
	espera.Wait()
}

func (p *processo) encerrar() {
	ctx, cancelar := context.WithTimeout(context.Background(), time.Second)
	p.conn.chamar(ctx, "", "Browser.close", nil, nil)
	cancelar()
	select {
	case <-p.fim:
	case <-time.After(3 * time.Second):
		if p.matar != nil {
			p.matar()
		}
		<-p.fim
	}
}

// pastaDoPerfil é a pasta de perfil do navegador (0700).
func (g *Gerente) pastaDoPerfil(perfil int64) (string, error) {
	dir := filepath.Join(g.Dir, strconv.FormatInt(perfil, 10))
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	for _, d := range []string{g.Dir, dir} {
		if err := os.Chmod(d, 0o700); err != nil {
			return "", err
		}
	}
	return dir, nil
}

// Argumentos do Chrome: controle só pelo pipe, perfil próprio, sem tela de
// boas-vindas. Sem --no-sandbox. O --simulate-outdated-no-au com data longe
// esconde o balão "Não é possível atualizar o Chrome", que cobre a página ao
// lado do agente; o Chrome do dia a dia da pessoa continua avisando.
func (g *Gerente) argumentos(dir string, geo Geometria) []string {
	args := []string{"--remote-debugging-pipe", "--user-data-dir=" + dir, "--no-first-run", "--no-default-browser-check",
		"--no-startup-window", "--disable-features=Translate", "--password-store=basic",
		"--simulate-outdated-no-au=Tue, 31 Dec 2099 23:59:59 GMT",
		fmt.Sprintf("--window-position=%d,%d", geo.X, geo.Y), fmt.Sprintf("--window-size=%d,%d", geo.Largura, geo.Altura)}
	if g.Headless {
		args = append(args, "--headless=new")
	}
	return args
}

func (g *Gerente) iniciarChrome(perfil int64, geo Geometria) (*processo, error) {
	executavel, _, err := Encontrar()
	if err != nil {
		return nil, err
	}
	dir, err := g.pastaDoPerfil(perfil)
	if err != nil {
		return nil, err
	}
	aberto, err := abrirChrome(executavel, g.argumentos(dir, geo))
	if err != nil {
		return nil, fmt.Errorf("abrindo o navegador: %w", err)
	}
	nucleoLe, nucleoEscreve := aberto.le, aberto.escreve
	p := &processo{conn: novaConexao(nucleoLe, nucleoEscreve), matar: aberto.matar, perfil: perfil, janelas: map[int64]*janela{}, preparando: map[string]*janela{}, fim: make(chan struct{})}
	go func() {
		aberto.esperar()
		nucleoEscreve.Close()
		nucleoLe.Close()
		p.conn.fechar(errFechado)
		close(p.fim)
	}()
	// Se o navegador abriu uma janela mesmo com --no-startup-window, a
	// primeira aba serve para a primeira tarefa.
	ctx, cancelar := context.WithTimeout(context.Background(), 15*time.Second)
	defer cancelar()
	var alvos struct {
		Lista []struct {
			ID   string `json:"targetId"`
			Tipo string `json:"type"`
		} `json:"targetInfos"`
	}
	if err := p.conn.chamar(ctx, "", "Target.getTargets", nil, &alvos); err != nil {
		p.encerrar()
		return nil, fmt.Errorf("o navegador não respondeu pelo pipe: %w", err)
	}
	for _, a := range alvos.Lista {
		if a.Tipo == "page" {
			p.sobra = a.ID
			break
		}
	}
	p.conn.chamar(ctx, "", "Target.setDiscoverTargets", map[string]any{"discover": true}, nil)
	return p, nil
}
