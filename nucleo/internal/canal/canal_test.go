//go:build unix

package canal

import (
	"errors"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"testing"
)

func permissao(t *testing.T, caminho string) os.FileMode {
	t.Helper()
	info, err := os.Stat(caminho)
	if err != nil {
		t.Fatal(err)
	}
	return info.Mode().Perm()
}

func TestAbrirCriaCanalFechadoParaOutros(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "colmeia")
	t.Setenv("COLMEIA_DIR", dir)

	_, token, fechar, err := Abrir()
	if err != nil {
		t.Fatal(err)
	}
	if got := permissao(t, dir); got != 0o700 {
		t.Errorf("diretório com permissão %o, esperado 700", got)
	}
	if got := permissao(t, filepath.Join(dir, NomeSocket)); got != 0o600 {
		t.Errorf("socket com permissão %o, esperado 600", got)
	}
	if got := permissao(t, filepath.Join(dir, NomeToken)); got != 0o600 {
		t.Errorf("token com permissão %o, esperado 600", got)
	}
	gravado, _ := os.ReadFile(filepath.Join(dir, NomeToken))
	if string(gravado) != token || len(token) != 64 {
		t.Errorf("token gravado não confere (%d caracteres)", len(token))
	}

	// Um segundo núcleo não pode tomar o canal de um que está rodando.
	if _, _, _, err := Abrir(); !errors.Is(err, ErrEmUso) {
		t.Errorf("segundo Abrir: esperado ErrEmUso, veio %v", err)
	}

	fechar()
	for _, nome := range []string{NomeSocket, NomeToken} {
		if _, err := os.Stat(filepath.Join(dir, nome)); !os.IsNotExist(err) {
			t.Errorf("%s continuou existindo depois de fechar", nome)
		}
	}
}

func TestAbrirRecusaDiretorioAbertoParaOutros(t *testing.T) {
	dir := filepath.Join(t.TempDir(), "colmeia")
	if err := os.Mkdir(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("COLMEIA_DIR", dir)
	// prepararDiretorio corrige a permissão para 0700 antes de conferir.
	_, _, fechar, err := Abrir()
	if err != nil {
		t.Fatal(err)
	}
	defer fechar()
	if got := permissao(t, dir); got != 0o700 {
		t.Errorf("diretório ficou com permissão %o", got)
	}
}

func TestExigirToken(t *testing.T) {
	protegido := ExigirToken("segredo", http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusOK)
	}))
	casos := []struct {
		nome, cabecalho string
		esperado        int
	}{
		{"sem token", "", http.StatusUnauthorized},
		{"token errado", "Bearer outro", http.StatusUnauthorized},
		{"sem Bearer", "segredo", http.StatusUnauthorized},
		{"token certo", "Bearer segredo", http.StatusOK},
	}
	for _, c := range casos {
		t.Run(c.nome, func(t *testing.T) {
			r := httptest.NewRequest("GET", "/v1/versao", nil)
			if c.cabecalho != "" {
				r.Header.Set("Authorization", c.cabecalho)
			}
			w := httptest.NewRecorder()
			protegido.ServeHTTP(w, r)
			if w.Code != c.esperado {
				t.Errorf("status %d, esperado %d", w.Code, c.esperado)
			}
		})
	}
}
