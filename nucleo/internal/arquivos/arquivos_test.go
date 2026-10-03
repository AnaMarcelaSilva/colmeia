//go:build unix

package arquivos

import (
	"bytes"
	"errors"
	"image"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
)

func pastaDeTeste(t *testing.T) (string, string) {
	t.Helper()
	base := t.TempDir()
	pasta := filepath.Join(base, "loja-web")
	fora := filepath.Join(base, "fora")
	for _, d := range []string{pasta, fora, filepath.Join(pasta, "src"), filepath.Join(pasta, ".git"), filepath.Join(pasta, "node_modules", "x")} {
		os.MkdirAll(d, 0o700)
	}
	escrever := func(caminho, conteudo string) {
		if err := os.WriteFile(caminho, []byte(conteudo), 0o600); err != nil {
			t.Fatal(err)
		}
	}
	escrever(filepath.Join(pasta, "README.md"), "# loja-web\nlinha 2\n")
	escrever(filepath.Join(pasta, "src", "main.go"), "package main\n")
	escrever(filepath.Join(pasta, ".git", "config"), "[core]\n")
	escrever(filepath.Join(pasta, ".env"), "SENHA=x\n")
	escrever(filepath.Join(pasta, "dados.bin"), "abc\x00def")
	escrever(filepath.Join(fora, "segredo.txt"), "não pode ler")
	os.Symlink(filepath.Join(fora, "segredo.txt"), filepath.Join(pasta, "link-fora"))
	os.Symlink("README.md", filepath.Join(pasta, "link-dentro"))
	return pasta, fora
}

func TestListarUmNivel(t *testing.T) {
	pasta, _ := pastaDeTeste(t)
	lista, mais, err := Listar(pasta, "")
	if err != nil || mais {
		t.Fatal(err, mais)
	}
	var nomes []string
	for _, e := range lista {
		nomes = append(nomes, e.Nome)
	}
	if strings.Join(nomes, ",") != ".git,node_modules,src,.env,dados.bin,link-dentro,link-fora,README.md" {
		t.Errorf("ordem: %v", nomes)
	}
	marcas := map[string]Entrada{}
	for _, e := range lista {
		marcas[e.Nome] = e
	}
	if !marcas[".git"].Ignorada || !marcas["node_modules"].Ignorada || marcas["src"].Ignorada {
		t.Errorf("ignoradas: %+v", lista)
	}
	if !marcas["link-fora"].Link || marcas["link-fora"].Pasta || !marcas[".env"].Sensivel || marcas["README.md"].Bytes != 19 {
		t.Errorf("marcas: %+v", lista)
	}
	if sub, _, err := Listar(pasta, "src"); err != nil || len(sub) != 1 || sub[0].Nome != "main.go" {
		t.Errorf("src: %+v %v", sub, err)
	}
	if sub, _, err := Listar(pasta, filepath.Join(pasta, "src")); err != nil || len(sub) != 1 {
		t.Errorf("src por caminho absoluto: %+v %v", sub, err)
	}
}

func TestCaminhosForaDaPastaSaoRecusados(t *testing.T) {
	pasta, fora := pastaDeTeste(t)
	for _, c := range []struct {
		caminho string
		erro    error
	}{
		{"../fora/segredo.txt", ErrForaDaPasta},
		{filepath.Join(fora, "segredo.txt"), ErrForaDaPasta},
		{"/etc/passwd", ErrForaDaPasta},
		{"src/../../fora/segredo.txt", ErrForaDaPasta},
		{"link-fora", ErrForaDaPasta},
		{".git/config", ErrIgnorada},
		{"node_modules/x", ErrIgnorada},
		{"src", ErrNaoArquivo},
		{"nao-existe.txt", ErrNaoExiste},
	} {
		if _, err := Ver(pasta, c.caminho, true); !errors.Is(err, c.erro) {
			t.Errorf("%s: %v, esperado %v", c.caminho, err, c.erro)
		}
	}
	if _, _, err := Listar(pasta, ".git"); !errors.Is(err, ErrIgnorada) {
		t.Errorf("listar .git: %v", err)
	}
	if _, _, err := Listar(pasta, ".."); !errors.Is(err, ErrForaDaPasta) {
		t.Errorf("listar ..: %v", err)
	}
	if _, _, err := Listar(filepath.Join(pasta, "sumiu"), ""); !errors.Is(err, ErrSemPasta) {
		t.Errorf("pasta apagada: %v", err)
	}
	// Um FIFO não trava a leitura.
	if err := syscall.Mkfifo(filepath.Join(pasta, "fila"), 0o600); err == nil {
		if _, err := Ver(pasta, "fila", true); !errors.Is(err, ErrNaoArquivo) {
			t.Errorf("fifo: %v", err)
		}
	}
}

func TestPreviaDeTextoBinarioESensivel(t *testing.T) {
	pasta, _ := pastaDeTeste(t)
	p, err := Ver(pasta, "README.md", false)
	if err != nil || p.Tipo != "texto" || p.Linhas != 2 || p.Cortado || p.Absoluto != filepath.Join(pasta, "README.md") {
		t.Errorf("texto: %+v %v", p, err)
	}
	if p, _ := Ver(pasta, "link-dentro", false); p.Tipo != "texto" {
		t.Errorf("link dentro da pasta: %+v", p)
	}
	if p, _ := Ver(pasta, "dados.bin", false); p.Tipo != "binario" || p.Texto != "" {
		t.Errorf("binário: %+v", p)
	}
	if p, _ := Ver(pasta, ".env", false); p.Tipo != "sensivel" || p.Texto != "" {
		t.Errorf("sensível sem pedir: %+v", p)
	}
	if p, _ := Ver(pasta, ".env", true); p.Tipo != "texto" || !p.Sensivel {
		t.Errorf("sensível pedindo: %+v", p)
	}
	grande := strings.Repeat("linha\n", MaxTexto/6+100)
	os.WriteFile(filepath.Join(pasta, "grande.log"), []byte(grande), 0o600)
	if p, _ := Ver(pasta, "grande.log", false); !p.Cortado || len(p.Texto) != MaxTexto {
		t.Errorf("grande: cortado=%v tamanho=%d", p.Cortado, len(p.Texto))
	}
}

func TestImagemReduzida(t *testing.T) {
	pasta, _ := pastaDeTeste(t)
	var b bytes.Buffer
	png.Encode(&b, image.NewNRGBA(image.Rect(0, 0, 3000, 1000)))
	os.WriteFile(filepath.Join(pasta, "tela.png"), b.Bytes(), 0o600)
	p, err := Ver(pasta, "tela.png", false)
	if err != nil || p.Tipo != "imagem" || p.Largura != 3000 || p.Formato != "PNG" {
		t.Fatalf("prévia da imagem: %+v %v", p, err)
	}
	bruto, err := Imagem(pasta, "tela.png", LadoPrevia)
	if err != nil {
		t.Fatal(err)
	}
	c, _ := png.DecodeConfig(bytes.NewReader(bruto))
	if c.Width > LadoPrevia || c.Height > LadoPrevia {
		t.Errorf("imagem não reduzida: %dx%d", c.Width, c.Height)
	}
	if _, err := Imagem(pasta, "README.md", LadoPrevia); !errors.Is(err, ErrNaoImagem) {
		t.Errorf("texto como imagem: %v", err)
	}
	if _, tipo, err := LerParaAnexo(pasta, "tela.png"); err != nil || tipo != "image/png" {
		t.Errorf("anexo: %s %v", tipo, err)
	}
	if _, _, err := LerParaAnexo(pasta, "link-fora"); !errors.Is(err, ErrForaDaPasta) {
		t.Errorf("anexo de fora: %v", err)
	}
}
