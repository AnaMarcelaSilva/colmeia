//go:build windows

package canal

import (
	"errors"
	"net"
)

// No Windows o canal será um named pipe com ACL só do usuário (ainda não feito).
// Até lá o núcleo recusa iniciar, em vez de cair para uma porta de rede.
func Abrir() (net.Listener, string, func(), error) {
	return nil, "", nil, errors.New("canal local no Windows ainda não implementado (named pipe)")
}

func conferirDono(string) error { return nil }
