//go:build windows

package canal

import (
	"errors"
	"net"
	"path/filepath"
	"testing"

	"golang.org/x/sys/windows"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
)

func TestCanalNoWindowsSoDoUsuario(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "canal")
	t.Setenv("COLMEIA_DIR", dir)
	l, token, fechar, err := Abrir()
	if err != nil {
		t.Fatal(err)
	}
	defer fechar()
	if len(token) != 64 {
		t.Errorf("token com %d caracteres", len(token))
	}
	go func() {
		if c, err := l.Accept(); err == nil {
			c.Close()
		}
	}()
	c, err := net.Dial("unix", filepath.Join(dir, NomeSocket))
	if err != nil {
		t.Fatalf("conectando no socket: %v", err)
	}
	c.Close()
	if _, _, _, err := Abrir(); !errors.Is(err, ErrEmUso) {
		t.Errorf("segundo núcleo no mesmo canal: %v", err)
	}
	// A lista de acesso é protegida (não herda) e só tem o usuário e o sistema.
	sd, err := windows.GetNamedSecurityInfo(dir, windows.SE_FILE_OBJECT, windows.DACL_SECURITY_INFORMATION)
	if err != nil {
		t.Fatal(err)
	}
	controle, _, err := sd.Control()
	if err != nil || controle&windows.SE_DACL_PROTECTED == 0 {
		t.Errorf("a lista de acesso deveria ser protegida: %v", err)
	}
	if texto := sd.String(); !containsSoUsuarioESistema(texto) {
		t.Errorf("lista de acesso: %s", texto)
	}
	for _, c := range []string{dir, filepath.Join(dir, NomeToken)} {
		if so, err := protecao.SoDoUsuario(c); !so {
			t.Errorf("%s aberto para outros: %v", c, err)
		}
	}
}

// containsSoUsuarioESistema confere que nenhuma entrada dá acesso a
// "todos" (WD), usuários (BU) ou usuários autenticados (AU).
func containsSoUsuarioESistema(sddl string) bool {
	for _, outro := range []string{";;;WD)", ";;;BU)", ";;;AU)"} {
		if contains(sddl, outro) {
			return false
		}
	}
	return contains(sddl, ";;;SY)")
}

func contains(s, sub string) bool {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return true
		}
	}
	return false
}
