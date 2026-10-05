//go:build windows

package canal

import (
	"fmt"
	"net"
	"os"
	"path/filepath"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
)

// Abrir prepara o diretório, gera o token e começa a escutar no socket. O
// Windows 10 (1803) e o 11 têm sockets Unix (AF_UNIX): o canal é o mesmo do
// Linux, sem porta de rede. O diretório tem uma lista de acesso só do
// usuário e do sistema, e o socket e o token herdam essa lista.
// Fechar remove o socket e o token.
func Abrir() (l net.Listener, token string, fechar func(), err error) {
	dir, err := Diretorio()
	if err != nil {
		return nil, "", nil, err
	}
	if err := prepararDiretorio(dir); err != nil {
		return nil, "", nil, fmt.Errorf("preparando %s: %w", dir, err)
	}
	caminho := filepath.Join(dir, NomeSocket)

	// Um socket que sobrou de uma execução anterior é removido; um que ainda
	// responde é de outro núcleo em execução, e esse não pode ser derrubado.
	if _, err := os.Lstat(caminho); err == nil {
		if c, err := net.DialTimeout("unix", caminho, 300*time.Millisecond); err == nil {
			c.Close()
			return nil, "", nil, ErrEmUso
		}
		if err := os.Remove(caminho); err != nil {
			return nil, "", nil, err
		}
	}
	l, err = net.Listen("unix", caminho)
	if err != nil {
		return nil, "", nil, err
	}
	if ul, ok := l.(*net.UnixListener); ok {
		ul.SetUnlinkOnClose(false)
	}
	token, err = NovoToken()
	if err == nil {
		err = gravarToken(dir, token)
	}
	if err != nil {
		l.Close()
		os.Remove(caminho)
		return nil, "", nil, err
	}
	fechar = func() {
		l.Close()
		// O token no arquivo diz de quem é o canal agora: um núcleo novo que
		// subiu enquanto este encerrava já pôs o dele no mesmo caminho.
		arquivoToken := filepath.Join(dir, NomeToken)
		if lido, err := os.ReadFile(arquivoToken); err != nil || string(lido) != token {
			return
		}
		os.Remove(caminho)
		os.Remove(arquivoToken)
	}
	return l, token, fechar, nil
}

// conferirDono recusa um diretório de outro usuário.
func conferirDono(dir string) error { return protecao.ConferirDono(dir) }
