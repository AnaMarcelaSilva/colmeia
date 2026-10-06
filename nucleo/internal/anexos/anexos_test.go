package anexos

import (
	"bytes"
	"encoding/binary"
	"hash/crc32"
	"image"
	"image/color"
	"image/jpeg"
	"image/png"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
)

func pngDeTeste(t *testing.T, l, a int) []byte {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, l, a))
	img.Set(1, 1, color.RGBA{200, 100, 50, 255})
	var b bytes.Buffer
	if err := png.Encode(&b, img); err != nil {
		t.Fatal(err)
	}
	return b.Bytes()
}

// pedaco monta um chunk PNG com o CRC certo.
func pedaco(tipo string, dados []byte) []byte {
	var b bytes.Buffer
	binary.Write(&b, binary.BigEndian, uint32(len(dados)))
	b.WriteString(tipo)
	b.Write(dados)
	binary.Write(&b, binary.BigEndian, crc32.ChecksumIEEE(append([]byte(tipo), dados...)))
	return b.Bytes()
}

func TestGravaSemMetadadosEComPermissaoSoDoUsuario(t *testing.T) {
	original := pngDeTeste(t, 20, 10)
	// Um tEXt com um caminho logo depois do IHDR (8 de assinatura + 25 do IHDR).
	comTexto := append(append(append([]byte{}, original[:33]...), pedaco("tEXt", []byte("Comment\x00/home/alguem/segredo"))...), original[33:]...)
	dir := filepath.Join(t.TempDir(), "anexos", "1")
	img, err := Gravar(dir, bytes.NewReader(comTexto))
	if err != nil {
		t.Fatal(err)
	}
	if img.Largura != 20 || img.Altura != 10 {
		t.Errorf("dimensões %dx%d", img.Largura, img.Altura)
	}
	gravado, _ := os.ReadFile(img.Caminho)
	if bytes.Contains(gravado, []byte("segredo")) {
		t.Error("os metadados continuaram no arquivo")
	}
	for _, c := range []string{img.Caminho, dir, filepath.Dir(dir)} {
		if so, err := protecao.SoDoUsuario(c); err != nil || !so {
			t.Errorf("%s aberto para outros (%v)", c, err)
		}
	}
	// A mesma imagem de novo reaproveita o arquivo.
	deNovo, err := Gravar(dir, bytes.NewReader(original))
	if err != nil || deNovo.Caminho != img.Caminho {
		t.Errorf("mesma imagem: %+v %v", deNovo, err)
	}
	if arquivos, _ := os.ReadDir(dir); len(arquivos) != 1 {
		t.Errorf("sobraram %d arquivos na pasta", len(arquivos))
	}
}

func TestRecusaOQueNaoServe(t *testing.T) {
	dir := t.TempDir()
	if _, err := Gravar(dir, bytes.NewReader([]byte("GIF89a não é png"))); err != ErrNaoPNG {
		t.Errorf("não-PNG: %v", err)
	}
	if _, err := Gravar(dir, bytes.NewReader(make([]byte, MaxBytes+10))); err != ErrGrande {
		t.Errorf("acima de 8 MB: %v", err)
	}
	// Bomba de descompressão: o cabeçalho diz 50000×50000, recusado antes de decodificar.
	ihdr := make([]byte, 13)
	binary.BigEndian.PutUint32(ihdr[0:], 50000)
	binary.BigEndian.PutUint32(ihdr[4:], 50000)
	ihdr[8], ihdr[9] = 8, 6
	bomba := append([]byte("\x89PNG\r\n\x1a\n"), pedaco("IHDR", ihdr)...)
	bomba = append(bomba, pedaco("IEND", nil)...)
	if _, err := Gravar(dir, bytes.NewReader(bomba)); err != ErrDimensoes {
		t.Errorf("bomba de descompressão: %v", err)
	}
}

func TestCaminhoSoComHash(t *testing.T) {
	if _, err := Caminho("/x", "../../etc/passwd", "png"); err == nil {
		t.Error("caminho fora do padrão aceito")
	}
	sha := strings.Repeat("a", 64)
	for _, formato := range []string{"desktop", "sh", "", "png/../x"} {
		if _, err := Caminho("/x", sha, formato); err == nil {
			t.Errorf("extensão %q aceita", formato)
		}
	}
	if c, err := Caminho("/x", sha, "mp4"); err != nil || c != filepath.Join("/x", sha+".mp4") {
		t.Errorf("mp4: %q, %v", c, err)
	}
}

// jpegComExif monta um JPEG com um segmento APP1 (EXIF) logo depois do SOI,
// como uma foto de celular, com um texto que não pode sobrar no PNG.
func jpegComExif(t *testing.T, l, a int) []byte {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, l, a))
	for y := 0; y < a; y++ {
		for x := 0; x < l; x++ {
			img.Set(x, y, color.RGBA{uint8(x), uint8(y), 120, 255})
		}
	}
	var b bytes.Buffer
	if err := jpeg.Encode(&b, img, &jpeg.Options{Quality: 90}); err != nil {
		t.Fatal(err)
	}
	exif := append([]byte("Exif\x00\x00"), []byte("GPS -23.5505,-46.6333 CAMERA-SECRETA")...)
	app1 := []byte{0xff, 0xe1, byte((len(exif) + 2) >> 8), byte(len(exif) + 2)}
	bruto := b.Bytes()
	return append(append(append([]byte{}, bruto[:2]...), append(app1, exif...)...), bruto[2:]...)
}

func TestFotoJPEGViraPNGSemExif(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "anexos", "1")
	foto := jpegComExif(t, 64, 48)
	if !bytes.Contains(foto, []byte("CAMERA-SECRETA")) {
		t.Fatal("o JPEG de teste ficou sem o EXIF")
	}
	img, err := GravarImagem(dir, bytes.NewReader(foto), "image/jpeg")
	if err != nil {
		t.Fatal(err)
	}
	gravado, _ := os.ReadFile(img.Caminho)
	if !bytes.HasPrefix(gravado, []byte("\x89PNG")) || bytes.Contains(gravado, []byte("CAMERA-SECRETA")) || bytes.Contains(gravado, []byte("Exif")) {
		t.Error("a foto não virou um PNG limpo")
	}
	if img.Largura != 64 || img.Altura != 48 || filepath.Ext(img.Caminho) != ".png" {
		t.Errorf("imagem gravada: %+v", img)
	}
	if _, err := GravarImagem(dir, bytes.NewReader(pngDeTeste(t, 4, 4)), "image/jpeg"); err != ErrNaoJPEG {
		t.Errorf("PNG enviado como JPEG: %v", err)
	}
	if _, err := GravarImagem(dir, bytes.NewReader(foto), "image/gif"); err == nil {
		t.Error("tipo desconhecido aceito")
	}
}

func TestFotoGrandeEReduzida(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "anexos", "1")
	img, err := GravarImagem(dir, bytes.NewReader(jpegComExif(t, MaxLadoFoto+10, 100)), "image/jpeg")
	if err != nil {
		t.Fatal(err)
	}
	if img.Largura > MaxLadoFoto || img.Largura < MaxLadoFoto/2 || img.Altura != 50 {
		t.Errorf("foto reduzida para %d×%d", img.Largura, img.Altura)
	}
}

func mp4DeTeste(tamanho int) []byte {
	v := make([]byte, tamanho)
	copy(v, []byte{0, 0, 0, 0x18, 'f', 't', 'y', 'p', 'i', 's', 'o', 'm'})
	return v
}

func TestVideoComBytesCertos(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "anexos", "1")
	v, err := GravarVideo(dir, bytes.NewReader(mp4DeTeste(4096)), "video/mp4")
	if err != nil {
		t.Fatal(err)
	}
	so, err := protecao.SoDoUsuario(v.Caminho)
	if err != nil || !so || v.Bytes != 4096 || v.Formato != "mp4" || filepath.Ext(v.Caminho) != ".mp4" {
		t.Errorf("vídeo gravado: %+v, %v", v, err)
	}
	if so, err := protecao.SoDoUsuario(dir); err != nil || !so {
		t.Errorf("pasta aberta para outros (%v)", err)
	}
	webm := append([]byte{0x1a, 0x45, 0xdf, 0xa3}, make([]byte, 100)...)
	if _, err := GravarVideo(dir, bytes.NewReader(webm), "video/webm"); err != nil {
		t.Errorf("webm: %v", err)
	}
	if _, err := GravarVideo(dir, bytes.NewReader(webm), "video/mp4"); err != ErrTipoDeVideo {
		t.Errorf("webm enviado como mp4: %v", err)
	}
	if _, err := GravarVideo(dir, bytes.NewReader([]byte("[Desktop Entry]\nExec=rm -rf ~\n")), "video/mp4"); err != ErrNaoVideo {
		t.Errorf("arquivo .desktop aceito como vídeo: %v", err)
	}
	if _, err := GravarVideo(dir, bytes.NewReader([]byte("curto")), "video/mp4"); err != ErrNaoVideo {
		t.Errorf("arquivo curto: %v", err)
	}
	if _, err := GravarVideo(dir, bytes.NewReader(mp4DeTeste(100)), "application/x-sh"); err != ErrNaoVideo {
		t.Errorf("tipo desconhecido: %v", err)
	}
	sobras, _ := filepath.Glob(filepath.Join(dir, ".video-*"))
	if len(sobras) > 0 {
		t.Errorf("temporários sobraram: %v", sobras)
	}
}

// leitorInfinito devolve bytes sem fim depois do cabeçalho de um mp4.
type leitorInfinito struct{ lidos int }

func (l *leitorInfinito) Read(p []byte) (int, error) {
	cabecalho := mp4DeTeste(12)
	for i := range p {
		if l.lidos < len(cabecalho) {
			p[i] = cabecalho[l.lidos]
		}
		l.lidos++
	}
	return len(p), nil
}

func TestVideoAcimaDoLimite(t *testing.T) {
	if testing.Short() {
		t.Skip("copia 512 MB")
	}
	dir := filepath.Join(t.TempDir(), "anexos", "1")
	if _, err := GravarVideo(dir, &leitorInfinito{}, "video/mp4"); err != ErrVideoGrande {
		t.Errorf("vídeo sem fim: %v", err)
	}
	sobras, _ := filepath.Glob(filepath.Join(dir, ".video-*"))
	arquivos, _ := filepath.Glob(filepath.Join(dir, "*.mp4"))
	if len(sobras)+len(arquivos) > 0 {
		t.Errorf("o vídeo grande deixou arquivos: %v %v", sobras, arquivos)
	}
}

// leitorQuebrado falha no meio da cópia, como um disco cheio ou a conexão caindo.
type leitorQuebrado struct{ lidos int }

func (l *leitorQuebrado) Read(p []byte) (int, error) {
	if l.lidos > 1000 {
		return 0, os.ErrClosed
	}
	n := copy(p, mp4DeTeste(2000)[l.lidos:])
	l.lidos += n
	return n, nil
}

func TestVideoComErroNaoDeixaTemporario(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "anexos", "1")
	if _, err := GravarVideo(dir, &leitorQuebrado{}, "video/mp4"); err == nil {
		t.Error("erro na cópia passou")
	}
	sobras, _ := filepath.Glob(filepath.Join(dir, ".video-*"))
	if len(sobras) > 0 {
		t.Errorf("temporário sobrou: %v", sobras)
	}
}
