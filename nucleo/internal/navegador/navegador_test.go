//go:build unix

package navegador

import (
	"bufio"
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"image"
	"image/png"
	"io"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"
)

// chromeFalso responde aos comandos do CDP por um par de pipes, como o
// Chrome faria pelo --remote-debugging-pipe.
type chromeFalso struct {
	t        *testing.T
	entrada  io.ReadCloser // o que o núcleo escreve
	saida    io.WriteCloser
	mu       sync.Mutex
	metodos  []string
	captura  string
	endereco string
	enviando sync.Mutex
}

func (f *chromeFalso) enviar(m map[string]any) {
	bruto, _ := json.Marshal(m)
	f.enviando.Lock()
	defer f.enviando.Unlock()
	f.saida.Write(append(bruto, 0))
}

func (f *chromeFalso) rodar() {
	leitor := bufio.NewReader(f.entrada)
	for {
		bruto, err := leitor.ReadBytes(0)
		if err != nil {
			f.saida.Close()
			return
		}
		var m struct {
			ID     int64          `json:"id"`
			Metodo string         `json:"method"`
			Params map[string]any `json:"params"`
			Sessao string         `json:"sessionId"`
		}
		json.Unmarshal(bytes.TrimSuffix(bruto, []byte{0}), &m)
		f.mu.Lock()
		f.metodos = append(f.metodos, m.Metodo)
		f.mu.Unlock()
		resultado := map[string]any{}
		switch m.Metodo {
		case "Target.getTargets":
			resultado["targetInfos"] = []any{}
		case "Target.createTarget":
			resultado["targetId"] = "alvo-1"
		case "Target.attachToTarget":
			resultado["sessionId"] = "sessao-1"
		case "Browser.getWindowForTarget":
			resultado["windowId"] = 1
		case "Page.navigate":
			resultado["frameId"] = "f"
			if u, _ := m.Params["url"].(string); strings.Contains(u, "nao-existe") {
				resultado["errorText"] = "net::ERR_NAME_NOT_RESOLVED"
				break
			}
			f.mu.Lock()
			f.endereco, _ = m.Params["url"].(string)
			f.mu.Unlock()
			go f.enviar(map[string]any{"method": "Page.loadEventFired", "params": map[string]any{}, "sessionId": m.Sessao})
		case "Target.getTargetInfo":
			f.mu.Lock()
			resultado["targetInfo"] = map[string]any{"targetId": "alvo-1", "url": f.endereco}
			f.mu.Unlock()
		case "Page.captureScreenshot":
			resultado["data"] = f.captura
		case "Browser.close":
			f.enviar(map[string]any{"id": m.ID, "result": resultado})
			f.saida.Close()
			return
		case "Desconhecido":
			f.enviar(map[string]any{"id": m.ID, "error": map[string]any{"code": -32601, "message": "método desconhecido"}})
			continue
		}
		f.enviar(map[string]any{"id": m.ID, "result": resultado})
	}
}

func gerenteFalso(t *testing.T) (*Gerente, *chromeFalso, chan Mudanca) {
	t.Helper()
	g := NovoGerente(t.TempDir())
	mudancas := make(chan Mudanca, 4)
	g.AoMudar = func(m Mudanca) { mudancas <- m }
	falso := &chromeFalso{t: t}
	g.iniciar = func(perfil int64, _ Geometria) (*processo, error) {
		nucleoLe, chromeEscreve := io.Pipe()
		chromeLe, nucleoEscreve := io.Pipe()
		falso.entrada, falso.saida = chromeLe, chromeEscreve
		// Uma captura de 20 MB em base64: bem acima do buffer comum de leitura.
		grande := make([]byte, 15<<20)
		var b bytes.Buffer
		png.Encode(&b, image.NewGray(image.Rect(0, 0, 2, 2)))
		copy(grande, b.Bytes())
		falso.captura = base64.StdEncoding.EncodeToString(grande)
		go falso.rodar()
		p := &processo{conn: novaConexao(nucleoLe, nucleoEscreve), perfil: perfil, janelas: map[int64]*janela{}, preparando: map[string]*janela{}, fim: make(chan struct{})}
		go func() { <-p.conn.fim; close(p.fim) }()
		return p, nil
	}
	t.Cleanup(g.FecharTodos)
	return g, falso, mudancas
}

func TestNavegadorPeloPipe(t *testing.T) {
	g, falso, mudancas := gerenteFalso(t)
	ctx := context.Background()
	u := URL{Endereco: "http://localhost:5173/pedidos?token=x", Descricao: "localhost:5173/pedidos"}
	nova, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 12, URL: u, Geometria: &Geometria{X: 800, Y: 0, Largura: 800, Altura: 900}})
	if err != nil || !nova {
		t.Fatalf("abrir: %v %v", nova, err)
	}
	if info := g.Aberto(1, 12); !info.Aberto || info.Descricao != "localhost:5173/pedidos" {
		t.Errorf("aberto: %+v", info)
	}
	if nova, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 12}); err != nil || nova {
		t.Errorf("trazer para frente: %v %v", nova, err)
	}
	png, descricao, err := g.Capturar(ctx, 1, 12)
	if err != nil || len(png) != 15<<20 || descricao != "localhost:5173/pedidos" {
		t.Fatalf("captura: %d bytes, %q, %v", len(png), descricao, err)
	}
	if _, _, err := g.Capturar(ctx, 1, 99); !errors.Is(err, ErrFechado) {
		t.Errorf("captura de tarefa sem janela: %v", err)
	}
	if err := g.conexaoDe(1).chamar(ctx, "", "Desconhecido", nil, nil); err == nil || !strings.Contains(err.Error(), "método desconhecido") {
		t.Errorf("erro do navegador: %v", err)
	}

	// O usuário fecha a janela: vira aviso, e a tarefa fica sem janela.
	falso.enviar(map[string]any{"method": "Target.targetDestroyed", "params": map[string]any{"targetId": "alvo-1"}})
	select {
	case m := <-mudancas:
		if m.Tarefa != 12 || m.Tipo != "fechado" {
			t.Errorf("mudança: %+v", m)
		}
	case <-time.After(2 * time.Second):
		t.Fatal("o fechamento da janela não avisou")
	}
	if g.Aberto(1, 12).Aberto {
		t.Error("a janela fechada continuou aberta")
	}
	falso.mu.Lock()
	metodos := strings.Join(falso.metodos, ",")
	falso.mu.Unlock()
	for _, m := range []string{"Target.createTarget", "Target.attachToTarget", "Page.enable", "Browser.setWindowBounds", "Page.navigate", "Target.activateTarget"} {
		if !strings.Contains(metodos, m) {
			t.Errorf("faltou %s em %s", m, metodos)
		}
	}
}

// Um endereço que não abre: a janela nova não fica (nem na tela, nem como
// aberta); a que já existia continua aberta e não vem para frente.
func TestEnderecoQueNaoAbre(t *testing.T) {
	g, falso, mudancas := gerenteFalso(t)
	ctx := context.Background()
	ruim := URL{Endereco: "http://nao-existe/", Descricao: "nao-existe/"}
	if nova, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 12, URL: ruim}); err == nil || nova || !strings.Contains(err.Error(), "ERR_NAME_NOT_RESOLVED") {
		t.Fatalf("abrir endereço que não existe: %v %v", nova, err)
	}
	if g.Aberto(1, 12).Aberto {
		t.Error("a janela nova que falhou ficou como aberta")
	}
	falso.mu.Lock()
	metodos := strings.Join(falso.metodos, ",")
	falso.metodos = nil
	falso.mu.Unlock()
	if !strings.HasSuffix(metodos, "Page.navigate,Target.closeTarget") {
		t.Errorf("a janela que falhou não foi fechada: %s", metodos)
	}

	if _, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 12, URL: URL{Endereco: "http://localhost:5173/", Descricao: "localhost:5173/"}}); err != nil {
		t.Fatal(err)
	}
	falso.mu.Lock()
	falso.metodos = nil
	falso.mu.Unlock()
	if _, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 12, URL: ruim, Geometria: &Geometria{X: 800, Y: 0, Largura: 800, Altura: 900}}); err == nil {
		t.Fatal("o endereço que não existe abriu")
	}
	if info := g.Aberto(1, 12); !info.Aberto {
		t.Error("a janela que já existia fechou com o endereço ruim")
	}
	falso.mu.Lock()
	metodos = strings.Join(falso.metodos, ",")
	falso.mu.Unlock()
	if strings.Contains(metodos, "Target.closeTarget") || strings.Contains(metodos, "Target.activateTarget") {
		t.Errorf("a janela que já existia: %s", metodos)
	}
	select {
	case m := <-mudancas:
		t.Errorf("aviso sem o usuário fechar nada: %+v", m)
	default:
	}
}

// conexaoDe é só para os testes.
func (g *Gerente) conexaoDe(perfil int64) *conexao {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.perfis[perfil].conn
}

func TestValidarURL(t *testing.T) {
	pasta := t.TempDir()
	os.WriteFile(filepath.Join(pasta, "index.html"), []byte("<h1>oi</h1>"), 0o600)
	os.MkdirAll(filepath.Join(pasta, ".git"), 0o700)
	os.WriteFile(filepath.Join(pasta, ".git", "config"), []byte("x"), 0o600)
	fora := filepath.Join(t.TempDir(), "fora.html")
	os.WriteFile(fora, []byte("x"), 0o600)
	os.Symlink(fora, filepath.Join(pasta, "link.html"))

	aceitas := map[string]string{
		"http://localhost:5173/pedidos?token=abc":      "localhost:5173/pedidos",
		"HTTPS://exemplo.com.br/a/b#c":                 "exemplo.com.br/a/b",
		"file://" + filepath.Join(pasta, "index.html"): "arquivo index.html",
	}
	for bruto, descricao := range aceitas {
		u, err := ValidarURL(bruto, pasta)
		if err != nil || u.Descricao != descricao || strings.Contains(u.Descricao, "token") {
			t.Errorf("%s: %+v %v", bruto, u, err)
		}
	}
	for _, bruto := range []string{
		"javascript:alert(1)", "data:text/html,oi", "chrome://settings", "about:blank", "ftp://x.com/a",
		"http://usuario:senha@x.com/", "http:///semhost", "localhost:5173", "",
		"file://" + fora, "file://" + filepath.Join(pasta, "link.html"), "file://" + filepath.Join(pasta, ".git", "config"),
		"file://" + filepath.Join(pasta, "..", "x"), "file://outra-maquina/" + pasta + "/index.html", "http://x.com/\na",
	} {
		if _, err := ValidarURL(bruto, pasta); !errors.Is(err, ErrURL) {
			t.Errorf("%q aceita: %v", bruto, err)
		}
	}
	// Os arquivos que a gaveta trata como sensíveis não abrem no navegador.
	for _, nome := range []string{".env", ".env.local", "chave.pem", "id_rsa"} {
		os.WriteFile(filepath.Join(pasta, nome), []byte("SEGREDO=1"), 0o600)
		if _, err := ValidarURL("file://"+filepath.Join(pasta, nome), pasta); !errors.Is(err, ErrSensivel) {
			t.Errorf("%s aceito: %v", nome, err)
		}
		if err := ArquivoDaPasta("file://"+filepath.Join(pasta, nome), pasta); !errors.Is(err, ErrSensivel) {
			t.Errorf("%s pedido pela página aceito: %v", nome, err)
		}
	}
}

func TestArquivoDaPasta(t *testing.T) {
	pasta := t.TempDir()
	os.MkdirAll(filepath.Join(pasta, "node_modules", "lib"), 0o700)
	os.WriteFile(filepath.Join(pasta, "node_modules", "lib", "a.js"), []byte("1"), 0o600)
	os.WriteFile(filepath.Join(pasta, "app.js"), []byte("1"), 0o600)
	os.MkdirAll(filepath.Join(pasta, ".git"), 0o700)
	os.WriteFile(filepath.Join(pasta, ".git", "config"), []byte("x"), 0o600)
	fora := filepath.Join(t.TempDir(), "fora.txt")
	os.WriteFile(fora, []byte("x"), 0o600)
	os.Symlink(fora, filepath.Join(pasta, "link.txt"))
	for _, aceito := range []string{
		"file://" + filepath.Join(pasta, "app.js") + "?v=2",
		"file://" + filepath.Join(pasta, "node_modules", "lib", "a.js"),
		"file://localhost" + filepath.Join(pasta, "app.js"),
		"file://" + pasta + "/",
	} {
		if err := ArquivoDaPasta(aceito, pasta); err != nil {
			t.Errorf("%s recusado: %v", aceito, err)
		}
	}
	for _, recusado := range []string{
		"file://" + fora, "file://" + filepath.Join(pasta, "link.txt"), "file://" + filepath.Join(pasta, ".git", "config"),
		"file://" + filepath.Join(pasta, "..", filepath.Base(fora)), "file:///etc/passwd", "file://outra/" + pasta + "/app.js",
		"http://x.com/", "file://" + filepath.Join(pasta, "nao-existe.js"),
	} {
		if err := ArquivoDaPasta(recusado, pasta); err == nil {
			t.Errorf("%s aceito", recusado)
		}
	}
	for endereco, esperado := range map[string]string{
		"https://x.com/a?token=1":                  "x.com/a",
		"about:blank":                              "",
		"chrome-error://chromewebdata/":            "página de erro",
		"file://" + filepath.Join(pasta, "app.js"): "arquivo app.js",
	} {
		if d, err := descrever(endereco, pasta); err != nil || d != esperado {
			t.Errorf("%s: %q %v", endereco, d, err)
		}
	}
	for _, endereco := range []string{"file://" + fora, "data:text/html,oi", "chrome://settings", "javascript:1"} {
		if _, err := descrever(endereco, pasta); !errors.Is(err, ErrSaiuDaPasta) {
			t.Errorf("%s descrito: %v", endereco, err)
		}
	}
}

// Com COLMEIA_TESTE_NAVEGADOR=1 e um Chrome instalado, abre um arquivo da
// pasta num Chrome de verdade (headless) e captura.
func TestNavegadorDeVerdade(t *testing.T) {
	if os.Getenv("COLMEIA_TESTE_NAVEGADOR") != "1" {
		t.Skip("defina COLMEIA_TESTE_NAVEGADOR=1 para abrir um Chrome de verdade")
	}
	if _, _, err := Encontrar(); err != nil {
		t.Skip(err)
	}
	pasta := t.TempDir()
	os.WriteFile(filepath.Join(pasta, "index.html"), []byte(`<body style="background:#fc0"><h1>loja-web</h1></body>`), 0o600)
	g := NovoGerente(t.TempDir())
	g.Headless = os.Getenv("DISPLAY") == "" || os.Getenv("COLMEIA_TESTE_HEADLESS") == "1"
	defer g.FecharTodos()
	u, err := ValidarURL("file://"+filepath.Join(pasta, "index.html"), pasta)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	if _, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 1, Pasta: pasta, URL: u, Geometria: &Geometria{X: 100, Y: 50, Largura: 800, Altura: 600}}); err != nil {
		t.Fatal(err)
	}
	// Um endereço que não resolve numa janela nova: a janela não fica.
	if _, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 2, Pasta: pasta, URL: URL{Endereco: "http://.env/", Descricao: ".env/"}}); err == nil {
		t.Error("http://.env/ abriu")
	}
	if g.Aberto(1, 2).Aberto {
		t.Error("a janela que não chegou ao endereço ficou como aberta")
	}
	bruto, _, err := g.Capturar(ctx, 1, 1)
	if err != nil {
		t.Fatal(err)
	}
	img, err := png.Decode(bytes.NewReader(bruto))
	if err != nil || img.Bounds().Dx() < 100 {
		t.Fatalf("captura: %v %v", img.Bounds(), err)
	}

	// Uma página da pasta que vai sozinha para um arquivo de fora: o
	// navegador recusa, e a captura também (vale o endereço de agora).
	foraDir := t.TempDir()
	fora := filepath.Join(foraDir, "fora.html")
	os.WriteFile(fora, []byte(`<body style="margin:0;background:#f00">SEGREDO-FORA-DA-PASTA</body>`), 0o600)
	dentro := filepath.Join(pasta, "vermelho.html")
	os.WriteFile(dentro, []byte(`<body style="margin:0;background:#f00">dentro</body>`), 0o600)
	os.WriteFile(filepath.Join(pasta, "redir.html"), []byte(`<script>location.href="file://`+fora+`"</script>`), 0o600)
	moldura := func(alvo string) string {
		return `<body style="margin:0;background:#fff"><iframe src="file://` + alvo + `" style="border:0;width:100vw;height:100vh"></iframe></body>`
	}
	os.WriteFile(filepath.Join(pasta, "iframe-fora.html"), []byte(moldura(fora)), 0o600)
	os.WriteFile(filepath.Join(pasta, "iframe-dentro.html"), []byte(moldura(dentro)), 0o600)
	abrir := func(nome string) {
		t.Helper()
		u, err := ValidarURL("file://"+filepath.Join(pasta, nome), pasta)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := g.Abrir(ctx, Pedido{Perfil: 1, Tarefa: 1, Pasta: pasta, URL: u}); err != nil {
			t.Fatal(err)
		}
		time.Sleep(800 * time.Millisecond)
	}
	abrir("redir.html")
	if bruto, descricao, err := g.Capturar(ctx, 1, 1); err == nil {
		img, _ := png.Decode(bytes.NewReader(bruto))
		if vermelho(img) {
			t.Fatalf("a captura levou o arquivo de fora da pasta (%q)", descricao)
		}
	}
	vermelhoDe := func(nome string) bool {
		t.Helper()
		abrir(nome)
		bruto, _, err := g.Capturar(ctx, 1, 1)
		if err != nil {
			t.Fatalf("%s: %v", nome, err)
		}
		img, err := png.Decode(bytes.NewReader(bruto))
		if err != nil {
			t.Fatal(err)
		}
		return vermelho(img)
	}
	if !vermelhoDe("iframe-dentro.html") {
		t.Error("o iframe de dentro da pasta não apareceu (o teste não prova nada)")
	}
	if vermelhoDe("iframe-fora.html") {
		t.Error("o iframe de fora da pasta apareceu na captura")
	}
	if err := g.Fechar(ctx, 1, 1); err != nil {
		t.Error(err)
	}
	if dir := filepath.Join(g.Dir, "1"); permissao(t, dir) != 0o700 {
		t.Errorf("pasta do perfil com permissão %o", permissao(t, dir))
	}
}

// vermelho diz se o meio da imagem é vermelho puro.
func vermelho(img image.Image) bool {
	b := img.Bounds()
	r, g, bl, _ := img.At(b.Min.X+b.Dx()/2, b.Min.Y+b.Dy()/2).RGBA()
	return r>>8 > 200 && g>>8 < 60 && bl>>8 < 60
}

func permissao(t *testing.T, caminho string) os.FileMode {
	info, err := os.Stat(caminho)
	if err != nil {
		t.Fatal(err)
	}
	return info.Mode().Perm()
}

func TestGeometriaPequenaNaoFicaGuardada(t *testing.T) {
	g := NovoGerente(t.TempDir())
	g.Lembrar(1, Geometria{X: 1, Y: 0, Largura: 500, Altura: 88})
	if geo := g.geometria(1); geo.X != 960 || geo.Altura > 900 {
		t.Errorf("guardou a geometria pequena: %+v", geo)
	}
	g.Lembrar(1, Geometria{X: 1240, Y: 0, Largura: 360, Altura: 720})
	if geo := g.geometria(1); geo.X != 1240 || geo.Largura != 360 {
		t.Errorf("não guardou a geometria certa: %+v", geo)
	}
}
