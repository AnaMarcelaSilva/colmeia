//go:build unix

package canal

import (
	"errors"
	"fmt"
	"net"
	"os"
	"path/filepath"
	"syscall"
	"time"
)

// Abrir prepara o diretório, gera o token e começa a escutar no socket.
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

	// umask garante que o socket já nasce 0600, sem janela aberta até o chmod.
	antiga := syscall.Umask(0o177)
	l, err = net.Listen("unix", caminho)
	syscall.Umask(antiga)
	if err != nil {
		return nil, "", nil, err
	}
	// O socket só sai no fim se ainda for este: um núcleo novo que subiu
	// enquanto este encerrava já pôs o dele no mesmo caminho (e o Close
	// padrão apagaria o socket do outro).
	if ul, ok := l.(*net.UnixListener); ok {
		ul.SetUnlinkOnClose(false)
	}
	if err := os.Chmod(caminho, 0o600); err != nil {
		l.Close()
		os.Remove(caminho)
		return nil, "", nil, err
	}
	meu, err := os.Lstat(caminho)
	if err != nil {
		l.Close()
		return nil, "", nil, err
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
		// O token no arquivo diz de quem é o canal agora (o inode sozinho
		// não basta: o sistema reaproveita o número).
		arquivoToken := filepath.Join(dir, NomeToken)
		if lido, err := os.ReadFile(arquivoToken); err != nil || string(lido) != token {
			return
		}
		if agora, err := os.Lstat(caminho); err == nil && os.SameFile(meu, agora) {
			os.Remove(caminho)
		}
		os.Remove(arquivoToken)
	}
	return l, token, fechar, nil
}

// conferirDono recusa um diretório de outro usuário ou com acesso para outros.
func conferirDono(dir string) error {
	info, err := os.Stat(dir)
	if err != nil {
		return err
	}
	st, ok := info.Sys().(*syscall.Stat_t)
	if !ok {
		return errors.New("não foi possível conferir o dono do diretório")
	}
	if int(st.Uid) != os.Getuid() {
		return fmt.Errorf("%s pertence a outro usuário", dir)
	}
	if info.Mode().Perm()&0o077 != 0 {
		return fmt.Errorf("%s está aberto para outros usuários", dir)
	}
	return nil
}
