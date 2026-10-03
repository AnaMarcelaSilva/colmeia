package mcp

import (
	"bytes"
	"context"
	"encoding/json"
	"io"
	"net"
	"net/http"
	"time"
)

// ClienteSocket fala com o núcleo pelo socket local, com o token do agente.
type ClienteSocket struct {
	Token string
	http  *http.Client
}

// NovoClienteSocket prepara o cliente para o socket do núcleo.
func NovoClienteSocket(socket, token string) *ClienteSocket {
	return &ClienteSocket{Token: token, http: &http.Client{
		// O navegador pode levar uns segundos para abrir e carregar a página.
		Timeout: 60 * time.Second,
		Transport: &http.Transport{DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			var d net.Dialer
			return d.DialContext(ctx, "unix", socket)
		}},
	}}
}

// Pedir faz o pedido ao núcleo e devolve o status e o corpo (até 64 MB).
func (c *ClienteSocket) Pedir(ctx context.Context, metodo, caminho string, corpo any) (int, []byte, error) {
	var leitor io.Reader
	if corpo != nil {
		bruto, err := json.Marshal(corpo)
		if err != nil {
			return 0, nil, err
		}
		leitor = bytes.NewReader(bruto)
	}
	pedido, err := http.NewRequestWithContext(ctx, metodo, "http://colmeia"+caminho, leitor)
	if err != nil {
		return 0, nil, err
	}
	pedido.Header.Set("Authorization", "Bearer "+c.Token)
	if corpo != nil {
		pedido.Header.Set("Content-Type", "application/json")
	}
	resposta, err := c.http.Do(pedido)
	if err != nil {
		return 0, nil, err
	}
	defer resposta.Body.Close()
	bruto, err := io.ReadAll(io.LimitReader(resposta.Body, 64<<20))
	return resposta.StatusCode, bruto, err
}
