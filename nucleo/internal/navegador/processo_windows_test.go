//go:build windows

package navegador

import (
	"encoding/binary"
	"os"
	"path/filepath"
	"testing"

	"golang.org/x/sys/windows"
)

func TestCaminhoDoURLNoWindows(t *testing.T) {
	if c := caminhoDoURL("/C:/pasta/pagina.html"); c != `C:\pasta\pagina.html` {
		t.Errorf("caminho: %q", c)
	}
	if u := caminhoNoURL(`C:\pasta\pagina.html`); u != "/C:/pasta/pagina.html" {
		t.Errorf("url: %q", u)
	}
}

func TestArquivoDaPastaNoWindows(t *testing.T) {
	pasta := t.TempDir()
	os.WriteFile(filepath.Join(pasta, "a.html"), []byte("<p>oi</p>"), 0o600)
	dentro := "file://" + caminhoNoURL(filepath.Join(pasta, "a.html"))
	if err := ArquivoDaPasta(dentro, pasta); err != nil {
		t.Errorf("arquivo da pasta recusado: %v", err)
	}
	fora := "file://" + caminhoNoURL(filepath.Join(filepath.Dir(pasta), "outro.html"))
	if err := ArquivoDaPasta(fora, pasta); err == nil {
		t.Error("arquivo fora da pasta aceito")
	}
}

func TestReservaDosDescritores(t *testing.T) {
	b := reservaDescritores(windows.Handle(0x10), windows.Handle(0x20))
	if binary.LittleEndian.Uint32(b) != 5 || b[4+3] != 0x09 || b[4+4] != 0x09 || b[4] != 0 {
		t.Errorf("cabeçalho: %v", b[:9])
	}
	if h := binary.LittleEndian.Uint64(b[9+3*8:]); h != 0x10 {
		t.Errorf("descritor 3: %x", h)
	}
}
