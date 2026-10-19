// Pacote icone transforma a imagem escolhida para um workspace num PNG
// quadrado e pequeno, o único formato que fica gravado e vai para a tela.
package icone

import (
	"bytes"
	"errors"
	"image"
	"image/draw"
	_ "image/gif"
	_ "image/jpeg"
	"image/png"

	xdraw "golang.org/x/image/draw"
	_ "golang.org/x/image/webp"
)

const (
	// Lado do ícone gravado, em pixels: nítido no quadrado de 36 px da barra
	// lateral numa tela com o dobro de densidade e no diálogo.
	Lado = 128
	// MaxBytes da imagem enviada.
	MaxBytes = 20 << 20
	// MaxPixels da imagem enviada (largura × altura), para uma imagem
	// pequena no disco não ocupar memória demais ao abrir.
	MaxPixels = 40_000_000
)

var (
	ErrFormato = errors.New("a imagem precisa ser PNG, JPEG, GIF ou WebP")
	ErrGrande  = errors.New("a imagem é grande demais (até 20 MB e 40 megapixels)")
)

// Reduzir lê a imagem, corta o quadrado do meio e reduz para Lado × Lado.
func Reduzir(bruto []byte) ([]byte, error) {
	if len(bruto) > MaxBytes {
		return nil, ErrGrande
	}
	cfg, _, err := image.DecodeConfig(bytes.NewReader(bruto))
	if err != nil {
		return nil, ErrFormato
	}
	if cfg.Width <= 0 || cfg.Height <= 0 || int64(cfg.Width)*int64(cfg.Height) > MaxPixels {
		return nil, ErrGrande
	}
	img, _, err := image.Decode(bytes.NewReader(bruto))
	if err != nil {
		return nil, ErrFormato
	}
	b := img.Bounds()
	lado := min(b.Dx(), b.Dy())
	x, y := b.Min.X+(b.Dx()-lado)/2, b.Min.Y+(b.Dy()-lado)/2
	corte := image.Rect(x, y, x+lado, y+lado)
	destino := image.NewNRGBA(image.Rect(0, 0, Lado, Lado))
	if lado <= Lado {
		// Pequena: amplia sem inventar detalhe.
		xdraw.ApproxBiLinear.Scale(destino, destino.Bounds(), img, corte, draw.Src, nil)
	} else {
		xdraw.CatmullRom.Scale(destino, destino.Bounds(), img, corte, draw.Src, nil)
	}
	var saida bytes.Buffer
	if err := (&png.Encoder{CompressionLevel: png.BestCompression}).Encode(&saida, destino); err != nil {
		return nil, err
	}
	return saida.Bytes(), nil
}
