//go:build unix

package api

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/coder/websocket"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

func servidorDeTeste(t *testing.T, demo bool) *httptest.Server {
	t.Helper()
	var bytes atomic.Int64
	// `cat` devolve o que recebe: dá para conferir a ida e a volta.
	pty, err := terminal.Iniciar([]string{"cat"}, nil, t.TempDir(), terminal.TamanhoPadrao)
	if err != nil {
		t.Fatal(err)
	}
	s := terminal.NovaSessao(0, pty, &bytes)
	go s.Ler()
	t.Cleanup(func() { s.Fechar() })
	srv := httptest.NewServer((&Servidor{Sessoes: []*terminal.Sessao{s}, Bytes: &bytes, Versao: "teste", Demo: demo}).Rotas())
	t.Cleanup(srv.Close)
	return srv
}

func TestTerminalIdaEVolta(t *testing.T) {
	srv := servidorDeTeste(t, false)
	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	conn, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http")+"/v1/terminais/0?intervalo=16", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.CloseNow()
	if err := conn.Write(ctx, websocket.MessageBinary, []byte("ola colmeia\n")); err != nil {
		t.Fatal(err)
	}
	var recebido strings.Builder
	for !strings.Contains(recebido.String(), "ola colmeia") {
		_, dados, err := conn.Read(ctx)
		if err != nil {
			t.Fatalf("lendo: %v (recebido até agora: %q)", err, recebido.String())
		}
		recebido.Write(dados)
	}
}

func TestTerminalInexistente(t *testing.T) {
	srv := servidorDeTeste(t, false)
	for _, id := range []string{"1", "-1", "abc"} {
		r, err := http.Get(srv.URL + "/v1/terminais/" + id)
		if err != nil {
			t.Fatal(err)
		}
		r.Body.Close()
		if r.StatusCode != http.StatusNotFound {
			t.Errorf("terminal %s: status %d, esperado 404", id, r.StatusCode)
		}
	}
}

func TestCargaSoExisteNoModoDemo(t *testing.T) {
	semDemo := servidorDeTeste(t, false)
	r, err := http.Post(semDemo.URL+"/v1/demo/carga?modo=leve", "", nil)
	if err != nil {
		t.Fatal(err)
	}
	r.Body.Close()
	if r.StatusCode != http.StatusNotFound {
		t.Errorf("sem --demo: status %d, esperado 404", r.StatusCode)
	}

	comDemo := servidorDeTeste(t, true)
	r, err = http.Post(comDemo.URL+"/v1/demo/carga?modo=desconhecida", "", nil)
	if err != nil {
		t.Fatal(err)
	}
	r.Body.Close()
	if r.StatusCode != http.StatusBadRequest {
		t.Errorf("carga desconhecida: status %d, esperado 400", r.StatusCode)
	}
}

func TestOrigemDeOutroSiteERecusada(t *testing.T) {
	srv := servidorDeTeste(t, false)
	ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancelar()
	cabecalhos := http.Header{"Origin": []string{"https://site-malicioso.example"}}
	_, _, err := websocket.Dial(ctx, "ws"+strings.TrimPrefix(srv.URL, "http")+"/v1/terminais/0", &websocket.DialOptions{HTTPHeader: cabecalhos})
	if err == nil {
		t.Error("uma página de outro site conseguiu abrir o terminal")
	}
}
