package anexos

import (
	"bytes"
	"encoding/binary"
	"hash/crc32"
	"image"
	"image/color"
	"image/png"
	"os"
	"path/filepath"
	"testing"
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
	if info, _ := os.Stat(img.Caminho); info.Mode().Perm() != 0o600 {
		t.Errorf("arquivo com permissão %o", info.Mode().Perm())
	}
	for _, d := range []string{dir, filepath.Dir(dir)} {
		if info, _ := os.Stat(d); info.Mode().Perm() != 0o700 {
			t.Errorf("pasta %s com permissão %o", d, info.Mode().Perm())
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
	if _, err := Caminho("/x", "../../etc/passwd"); err == nil {
		t.Error("caminho fora do padrão aceito")
	}
}
