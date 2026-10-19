package icone

import (
	"bytes"
	"image"
	"image/color"
	"image/jpeg"
	"image/png"
	"testing"
)

func TestReduzCortaOMeioEmQuadrado(t *testing.T) {
	// 300 × 100: só o terço do meio (vermelho) fica.
	img := image.NewRGBA(image.Rect(0, 0, 300, 100))
	for x := range 300 {
		for y := range 100 {
			c := color.RGBA{0, 0, 255, 255}
			if x >= 100 && x < 200 {
				c = color.RGBA{255, 0, 0, 255}
			}
			img.Set(x, y, c)
		}
	}
	var bruto bytes.Buffer
	jpeg.Encode(&bruto, img, &jpeg.Options{Quality: 95})
	saida, err := Reduzir(bruto.Bytes())
	if err != nil {
		t.Fatal(err)
	}
	final, err := png.Decode(bytes.NewReader(saida))
	if err != nil {
		t.Fatal(err)
	}
	if final.Bounds().Dx() != Lado || final.Bounds().Dy() != Lado {
		t.Fatalf("lado %v", final.Bounds())
	}
	r, _, b, _ := final.At(Lado/2, Lado/2).RGBA()
	if r < 0xc000 || b > 0x4000 {
		t.Fatalf("o meio não ficou vermelho: r=%x b=%x", r, b)
	}
}

func TestRecusaOQueNaoEImagem(t *testing.T) {
	if _, err := Reduzir([]byte("<svg></svg>")); err != ErrFormato {
		t.Fatalf("esperava ErrFormato, veio %v", err)
	}
	if _, err := Reduzir(make([]byte, MaxBytes+1)); err != ErrGrande {
		t.Fatalf("esperava ErrGrande, veio %v", err)
	}
}
