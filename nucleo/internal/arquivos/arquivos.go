// Package arquivos lê a pasta de uma tarefa, só para mostrar: a árvore, um
// nível por vez, e a pré-visualização de um arquivo. Tudo passa por os.Root,
// que recusa sair da pasta (caminho absoluto, "..", link simbólico para
// fora); as pastas pesadas (.git, node_modules, target) aparecem, mas não
// abrem. Nada aqui escreve no disco.
package arquivos

import (
	"bytes"
	"errors"
	"image"
	"image/gif"
	"image/jpeg"
	"image/png"
	"io"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/anexos"
)

const (
	// MaxEntradas listadas de uma pasta.
	MaxEntradas = 2000
	// MaxTexto mostrado de um arquivo de texto.
	MaxTexto = 256 << 10
	// MaxImagem em bytes, para a pré-visualização.
	MaxImagem = 32 << 20
	// LadoPrevia: a imagem da pré-visualização é reduzida até caber nisso.
	LadoPrevia = 2048
	// olharBinario: um NUL nesse começo do arquivo faz dele binário.
	olharBinario = 8 << 10
)

var (
	ErrForaDaPasta  = errors.New("o caminho precisa ser de dentro da pasta da tarefa")
	ErrIgnorada     = errors.New("essa pasta não é listada, para não pesar")
	ErrSemPasta     = errors.New("A pasta da tarefa não existe mais")
	ErrSemPermissao = errors.New("Sem permissão para ler este arquivo")
	ErrNaoArquivo   = errors.New("não é um arquivo comum")
	ErrNaoImagem    = errors.New("o arquivo não é um PNG ou JPEG válido")
	ErrNaoExiste    = errors.New("o arquivo não existe")
)

// Ignoradas são as pastas que aparecem na árvore, mas não abrem.
var Ignoradas = map[string]bool{".git": true, "node_modules": true, "target": true}

// Entrada é um item da pasta.
type Entrada struct {
	Nome     string `json:"nome"`
	Pasta    bool   `json:"pasta"`
	Link     bool   `json:"link,omitempty"`
	Ignorada bool   `json:"ignorada,omitempty"`
	Bytes    int64  `json:"bytes"`
	Alterado string `json:"alterado"`
	Sensivel bool   `json:"sensivel,omitempty"`
}

// Relativo transforma o caminho pedido (relativo à pasta, ou absoluto de
// dentro dela) num caminho relativo limpo. Recusa sair da pasta e entrar
// nas pastas ignoradas. "" e "." são a própria pasta.
func Relativo(pasta, caminho string) (string, error) {
	if strings.ContainsRune(caminho, 0) {
		return "", ErrForaDaPasta
	}
	if filepath.IsAbs(caminho) {
		rel, err := filepath.Rel(filepath.Clean(pasta), filepath.Clean(caminho))
		if err != nil {
			return "", ErrForaDaPasta
		}
		caminho = rel
	}
	caminho = filepath.Clean(caminho)
	// No Windows também recusa "\\pasta" (começa na raiz da unidade) e "D:x".
	sep := string(filepath.Separator)
	if caminho == ".." || strings.HasPrefix(caminho, ".."+sep) || filepath.IsAbs(caminho) || strings.HasPrefix(caminho, sep) || filepath.VolumeName(caminho) != "" {
		return "", ErrForaDaPasta
	}
	for parte := range strings.SplitSeq(caminho, string(filepath.Separator)) {
		if Ignoradas[parte] {
			return "", ErrIgnorada
		}
	}
	return caminho, nil
}

// abrir abre a pasta da tarefa como raiz.
func abrir(pasta string) (*os.Root, error) {
	raiz, err := os.OpenRoot(pasta)
	if err != nil {
		return nil, traduzir(err, ErrSemPasta)
	}
	return raiz, nil
}

func traduzir(err error, naoExiste error) error {
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return naoExiste
	case errors.Is(err, fs.ErrPermission):
		return ErrSemPermissao
	}
	// Um link para fora da pasta (ou ".." que escapou) chega aqui como erro
	// do próprio os.Root.
	if strings.Contains(err.Error(), "escapes from parent") || strings.Contains(err.Error(), "path escapes") {
		return ErrForaDaPasta
	}
	return err
}

// Sensivel diz se o nome do arquivo costuma guardar senhas ou chaves: a tela
// pede confirmação antes de mostrar (pode estar compartilhada).
func Sensivel(nome string) bool {
	n := strings.ToLower(nome)
	return strings.HasPrefix(n, ".env") || strings.HasPrefix(n, "id_rsa") || strings.HasPrefix(n, "id_ed25519") || strings.HasPrefix(n, "id_ecdsa") ||
		strings.HasSuffix(n, ".pem") || strings.HasSuffix(n, ".key") || strings.HasSuffix(n, ".p12") || strings.HasSuffix(n, ".pfx") ||
		n == ".netrc" || n == ".pgpass" || n == "credentials"
}

// Listar lê um nível da pasta (rel, relativo à pasta da tarefa): pastas
// primeiro, por nome. Links simbólicos aparecem marcados e não são seguidos.
// mais diz que havia mais que MaxEntradas.
func Listar(pasta, caminho string) (lista []Entrada, mais bool, err error) {
	rel, err := Relativo(pasta, caminho)
	if err != nil {
		return nil, false, err
	}
	raiz, err := abrir(pasta)
	if err != nil {
		return nil, false, err
	}
	defer raiz.Close()
	info, err := raiz.Lstat(rel)
	if err != nil {
		return nil, false, traduzir(err, ErrNaoExiste)
	}
	if !info.IsDir() {
		return nil, false, ErrNaoArquivo
	}
	dir, err := raiz.Open(rel)
	if err != nil {
		return nil, false, traduzir(err, ErrNaoExiste)
	}
	defer dir.Close()
	entradas, err := dir.ReadDir(MaxEntradas + 1)
	if err != nil && !errors.Is(err, io.EOF) {
		return nil, false, traduzir(err, ErrNaoExiste)
	}
	if len(entradas) > MaxEntradas {
		entradas, mais = entradas[:MaxEntradas], true
	}
	lista = make([]Entrada, 0, len(entradas))
	for _, e := range entradas {
		item := Entrada{Nome: e.Name(), Pasta: e.IsDir(), Link: e.Type()&fs.ModeSymlink != 0}
		if info, err := e.Info(); err == nil {
			if !item.Pasta && !item.Link {
				item.Bytes = info.Size()
			}
			item.Alterado = info.ModTime().UTC().Format(time.RFC3339)
		}
		item.Ignorada = item.Pasta && Ignoradas[item.Nome]
		item.Sensivel = !item.Pasta && Sensivel(item.Nome)
		lista = append(lista, item)
	}
	slices.SortFunc(lista, func(a, b Entrada) int {
		if a.Pasta != b.Pasta {
			if a.Pasta {
				return -1
			}
			return 1
		}
		return strings.Compare(strings.ToLower(a.Nome), strings.ToLower(b.Nome))
	})
	return lista, mais, nil
}

// Previa é o que a tela mostra de um arquivo.
type Previa struct {
	Tipo     string `json:"tipo"` // texto, imagem, binario ou sensivel
	Nome     string `json:"nome"`
	Caminho  string `json:"caminho"`
	Absoluto string `json:"caminho_absoluto"`
	Bytes    int64  `json:"bytes"`
	Texto    string `json:"texto,omitempty"`
	Cortado  bool   `json:"cortado,omitempty"`
	Linhas   int    `json:"linhas,omitempty"`
	Largura  int    `json:"largura,omitempty"`
	Altura   int    `json:"altura,omitempty"`
	Formato  string `json:"formato,omitempty"`
	Sensivel bool   `json:"sensivel,omitempty"`
}

// abrirArquivo abre um arquivo comum da pasta (nunca um FIFO, um
// dispositivo ou uma pasta).
func abrirArquivo(pasta, caminho string) (*os.File, string, fs.FileInfo, error) {
	rel, err := Relativo(pasta, caminho)
	if err != nil {
		return nil, "", nil, err
	}
	if rel == "." {
		return nil, "", nil, ErrNaoArquivo
	}
	raiz, err := abrir(pasta)
	if err != nil {
		return nil, "", nil, err
	}
	defer raiz.Close()
	info, err := raiz.Stat(rel)
	if err != nil {
		return nil, "", nil, traduzir(err, ErrNaoExiste)
	}
	if !info.Mode().IsRegular() {
		return nil, "", nil, ErrNaoArquivo
	}
	f, err := raiz.Open(rel)
	if err != nil {
		return nil, "", nil, traduzir(err, ErrNaoExiste)
	}
	return f, rel, info, nil
}

// formatoImagem reconhece PNG, JPEG e GIF pelos primeiros bytes.
func formatoImagem(inicio []byte) string {
	switch {
	case bytes.HasPrefix(inicio, []byte("\x89PNG\r\n\x1a\n")):
		return "png"
	case bytes.HasPrefix(inicio, []byte{0xff, 0xd8, 0xff}):
		return "jpeg"
	case bytes.HasPrefix(inicio, []byte("GIF87a")), bytes.HasPrefix(inicio, []byte("GIF89a")):
		return "gif"
	}
	return ""
}

// Ver monta a pré-visualização. Um arquivo sensível só vem com o conteúdo
// se mostrar for true.
func Ver(pasta, caminho string, mostrar bool) (Previa, error) {
	f, rel, info, err := abrirArquivo(pasta, caminho)
	if err != nil {
		return Previa{}, err
	}
	defer f.Close()
	p := Previa{Nome: filepath.Base(rel), Caminho: rel, Absoluto: filepath.Join(pasta, rel), Bytes: info.Size(), Sensivel: Sensivel(filepath.Base(rel))}
	if p.Sensivel && !mostrar {
		p.Tipo = "sensivel"
		return p, nil
	}
	inicio := make([]byte, olharBinario)
	n, err := io.ReadFull(f, inicio)
	if err != nil && !errors.Is(err, io.EOF) && !errors.Is(err, io.ErrUnexpectedEOF) {
		return p, traduzir(err, ErrNaoExiste)
	}
	inicio = inicio[:n]
	if formato := formatoImagem(inicio); formato != "" {
		p.Tipo, p.Formato = "imagem", strings.ToUpper(formato)
		if config, _, err := image.DecodeConfig(io.MultiReader(bytes.NewReader(inicio), f)); err == nil {
			p.Largura, p.Altura = config.Width, config.Height
		}
		return p, nil
	}
	if bytes.IndexByte(inicio, 0) >= 0 {
		p.Tipo = "binario"
		return p, nil
	}
	resto, err := io.ReadAll(io.LimitReader(f, MaxTexto-int64(len(inicio))))
	if err != nil {
		return p, traduzir(err, ErrNaoExiste)
	}
	texto := append(inicio, resto...)
	p.Tipo = "texto"
	p.Cortado = info.Size() > int64(len(texto))
	p.Texto = strings.ToValidUTF8(string(texto), "�")
	p.Linhas = strings.Count(p.Texto, "\n")
	if !strings.HasSuffix(p.Texto, "\n") && p.Texto != "" {
		p.Linhas++
	}
	return p, nil
}

// Imagem lê uma imagem da pasta (PNG, JPEG ou GIF), confere as dimensões
// antes de decodificar e devolve um PNG reduzido até caber em lado.
func Imagem(pasta, caminho string, lado int) ([]byte, error) {
	f, _, info, err := abrirArquivo(pasta, caminho)
	if err != nil {
		return nil, err
	}
	defer f.Close()
	if info.Size() > MaxImagem {
		return nil, anexos.ErrGrande
	}
	bruto, err := io.ReadAll(io.LimitReader(f, MaxImagem+1))
	if err != nil {
		return nil, traduzir(err, ErrNaoExiste)
	}
	var config func(io.Reader) (image.Config, error)
	var decodificar func(io.Reader) (image.Image, error)
	switch formatoImagem(bruto) {
	case "png":
		config, decodificar = png.DecodeConfig, png.Decode
	case "jpeg":
		config, decodificar = jpeg.DecodeConfig, jpeg.Decode
	case "gif":
		config, decodificar = gif.DecodeConfig, gif.Decode
	default:
		return nil, ErrNaoImagem
	}
	c, err := config(bytes.NewReader(bruto))
	if err != nil {
		return nil, ErrNaoImagem
	}
	if c.Width <= 0 || c.Height <= 0 || c.Width > anexos.MaxLado || c.Height > anexos.MaxLado || c.Width*c.Height > anexos.MaxPixels {
		return nil, anexos.ErrDimensoes
	}
	img, err := decodificar(bytes.NewReader(bruto))
	if err != nil {
		return nil, ErrNaoImagem
	}
	var saida bytes.Buffer
	if err := png.Encode(&saida, anexos.Reduzir(img, lado)); err != nil {
		return nil, err
	}
	return saida.Bytes(), nil
}

// LerParaAnexo lê um PNG ou JPEG da pasta (até anexos.MaxBytes) e diz o tipo,
// para virar anexo da tarefa.
func LerParaAnexo(pasta, caminho string) ([]byte, string, error) {
	f, _, info, err := abrirArquivo(pasta, caminho)
	if err != nil {
		return nil, "", err
	}
	defer f.Close()
	if info.Size() > anexos.MaxBytes {
		return nil, "", anexos.ErrGrande
	}
	bruto, err := io.ReadAll(io.LimitReader(f, anexos.MaxBytes+1))
	if err != nil {
		return nil, "", traduzir(err, ErrNaoExiste)
	}
	if len(bruto) > anexos.MaxBytes {
		return nil, "", anexos.ErrGrande
	}
	switch formatoImagem(bruto) {
	case "png":
		return bruto, "image/png", nil
	case "jpeg":
		return bruto, "image/jpeg", nil
	}
	return nil, "", ErrNaoImagem
}

// Erro diz se o erro é um dos daqui, com mensagem para mostrar.
func Erro(err error) bool {
	for _, e := range []error{ErrForaDaPasta, ErrIgnorada, ErrSemPasta, ErrSemPermissao, ErrNaoArquivo, ErrNaoImagem, ErrNaoExiste,
		anexos.ErrGrande, anexos.ErrDimensoes} {
		if errors.Is(err, e) {
			return true
		}
	}
	return false
}
