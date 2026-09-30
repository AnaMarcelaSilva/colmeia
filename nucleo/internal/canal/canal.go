// Package canal abre o canal local entre o núcleo e as telas.
//
// O núcleo nunca escuta em porta de rede. No Linux e no macOS ele usa um
// socket Unix dentro de um diretório que só o usuário acessa (0700), e cada
// conexão precisa apresentar um token gerado a cada início (arquivo 0600).
// Assim nenhum outro usuário da máquina e nenhuma página aberta no navegador
// consegue falar com os terminais.
package canal

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/hex"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path/filepath"
	"strings"
)

const (
	NomeSocket = "nucleo.sock"
	NomeToken  = "token"
)

// Diretorio é onde ficam o socket e o token. COLMEIA_DIR substitui o padrão
// (usado nos testes); senão, $XDG_RUNTIME_DIR/colmeia ou o cache do usuário.
func Diretorio() (string, error) {
	if d := os.Getenv("COLMEIA_DIR"); d != "" {
		return d, nil
	}
	base := os.Getenv("XDG_RUNTIME_DIR")
	if base == "" {
		cache, err := os.UserCacheDir()
		if err != nil {
			return "", fmt.Errorf("sem diretório para o canal: %w", err)
		}
		base = cache
	}
	return filepath.Join(base, "colmeia"), nil
}

// prepararDiretorio cria o diretório com permissão 0700 e confere se ele é do
// usuário atual e não está aberto para outros.
func prepararDiretorio(dir string) error {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return err
	}
	if err := os.Chmod(dir, 0o700); err != nil {
		return err
	}
	return conferirDono(dir)
}

// NovoToken gera 32 bytes aleatórios em hexadecimal.
func NovoToken() (string, error) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		return "", err
	}
	return hex.EncodeToString(b), nil
}

// gravarToken escreve o token com permissão 0600, trocando o arquivo de uma vez
// para uma tela nunca ler um token pela metade.
func gravarToken(dir, token string) error {
	temporario, err := os.CreateTemp(dir, ".token-*")
	if err != nil {
		return err
	}
	defer os.Remove(temporario.Name())
	if err := temporario.Chmod(0o600); err != nil {
		temporario.Close()
		return err
	}
	if _, err := temporario.WriteString(token); err != nil {
		temporario.Close()
		return err
	}
	if err := temporario.Close(); err != nil {
		return err
	}
	return os.Rename(temporario.Name(), filepath.Join(dir, NomeToken))
}

// ExigirToken só deixa passar pedidos com "Authorization: Bearer <token>".
// A comparação é em tempo constante.
func ExigirToken(token string, proximo http.Handler) http.Handler {
	esperado := []byte("Bearer " + token)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		recebido := []byte(strings.TrimSpace(r.Header.Get("Authorization")))
		if subtle.ConstantTimeCompare(recebido, esperado) != 1 {
			http.Error(w, "não autorizado", http.StatusUnauthorized)
			return
		}
		proximo.ServeHTTP(w, r)
	})
}

var ErrEmUso = errors.New("já existe um núcleo rodando neste canal")
