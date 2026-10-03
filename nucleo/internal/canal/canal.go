// Package canal abre o canal local entre o núcleo e as telas.
//
// O núcleo nunca escuta em porta de rede. No Linux e no macOS ele usa um
// socket Unix dentro de um diretório que só o usuário acessa (0700), e cada
// conexão precisa apresentar um token gerado a cada início (arquivo 0600).
// Assim nenhum outro usuário da máquina e nenhuma página aberta no navegador
// consegue falar com os terminais.
package canal

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"crypto/subtle"
	"encoding/hex"
	"errors"
	"fmt"
	"net/http"
	"os"
	"path"
	"path/filepath"
	"strings"
	"sync"
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

// Quem diz de onde veio um pedido: da tela (Agente 0) ou de um agente que a
// Colmeia abriu, pelo token próprio dele.
type Quem struct{ Agente int64 }

type chaveQuem struct{}

// QuemPediu devolve quem fez o pedido. Fora de Autenticar não há ninguém.
func QuemPediu(ctx context.Context) (Quem, bool) {
	q, ok := ctx.Value(chaveQuem{}).(Quem)
	return q, ok
}

// ComQuem põe quem pediu no contexto (Autenticar e os testes).
func ComQuem(ctx context.Context, q Quem) context.Context {
	return context.WithValue(ctx, chaveQuem{}, q)
}

// Fichas guarda, só em memória, os tokens dos agentes: o sha256 do token
// aponta para o id do agente. O token em si não fica guardado em lugar
// nenhum do núcleo e nunca sai numa resposta.
type Fichas struct {
	mu        sync.Mutex
	porHash   map[[32]byte]int64
	porAgente map[int64][32]byte
}

func NovasFichas() *Fichas {
	return &Fichas{porHash: map[[32]byte]int64{}, porAgente: map[int64][32]byte{}}
}

// Emitir gera um token novo para o agente; o anterior, se houver, deixa de valer.
func (f *Fichas) Emitir(agente int64) (string, error) {
	token, err := NovoToken()
	if err != nil {
		return "", err
	}
	h := sha256.Sum256([]byte(token))
	f.mu.Lock()
	defer f.mu.Unlock()
	if antigo, ok := f.porAgente[agente]; ok {
		delete(f.porHash, antigo)
	}
	f.porHash[h] = agente
	f.porAgente[agente] = h
	return token, nil
}

// Revogar tira o token do agente (ele terminou ou foi removido).
func (f *Fichas) Revogar(agente int64) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if h, ok := f.porAgente[agente]; ok {
		delete(f.porHash, h)
		delete(f.porAgente, agente)
	}
}

// Agente diz de qual agente é o token. A busca é pelo hash: o tempo não
// depende de quanto do token confere.
func (f *Fichas) Agente(token string) (int64, bool) {
	if f == nil || token == "" {
		return 0, false
	}
	h := sha256.Sum256([]byte(token))
	f.mu.Lock()
	defer f.mu.Unlock()
	id, ok := f.porHash[h]
	return id, ok
}

// PrefixoAgente é onde ficam as rotas dos agentes. O token da tela não entra
// nelas, e o token de um agente só entra nelas.
const PrefixoAgente = "/v1/agente/"

// RotaDeAgente diz se o caminho é de uma rota dos agentes. Um caminho que o
// roteador ainda limparia (com "..", "//") não conta como de agente.
func RotaDeAgente(caminho string) bool {
	return strings.HasPrefix(caminho, PrefixoAgente) && path.Clean(caminho) == caminho
}

// Autenticar só deixa passar pedidos com "Authorization: Bearer <token>": o
// token principal (o da tela) vale para tudo menos as rotas dos agentes; o
// token de um agente vale só para elas. Quem pediu vai no contexto. A
// comparação do token principal é em tempo constante.
func Autenticar(token string, agentes *Fichas, proximo http.Handler) http.Handler {
	esperado := []byte("Bearer " + token)
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		recebido := strings.TrimSpace(r.Header.Get("Authorization"))
		deAgente := strings.HasPrefix(path.Clean(r.URL.Path)+"/", PrefixoAgente)
		if subtle.ConstantTimeCompare([]byte(recebido), esperado) == 1 {
			if deAgente {
				http.Error(w, "não autorizado", http.StatusUnauthorized)
				return
			}
			proximo.ServeHTTP(w, r.WithContext(ComQuem(r.Context(), Quem{})))
			return
		}
		valor, ok := strings.CutPrefix(recebido, "Bearer ")
		if id, achou := agentes.Agente(valor); ok && achou && RotaDeAgente(r.URL.Path) {
			proximo.ServeHTTP(w, r.WithContext(ComQuem(r.Context(), Quem{Agente: id})))
			return
		}
		http.Error(w, "não autorizado", http.StatusUnauthorized)
	})
}

// ExigirToken é Autenticar sem tokens de agentes.
func ExigirToken(token string, proximo http.Handler) http.Handler {
	return Autenticar(token, nil, proximo)
}

var ErrEmUso = errors.New("já existe um núcleo rodando neste canal")
