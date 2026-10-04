//go:build unix

package api

import (
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
	"time"

	"github.com/go-sql-driver/mysql"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/bancos"
)

// Senha recusada pelo servidor (trocada lá, ou conexão salva sem senha): a
// resposta marca senha_recusada para a tela oferecer "Trocar senha", e outros
// erros de conexão não (nem o 1044, senha certa sem acesso ao banco).
func TestErroDeSenhaRecusadaVaiMarcado(t *testing.T) {
	casos := []struct {
		nome    string
		err     error
		marcado bool
	}{
		{"mysql 1045", &mysql.MySQLError{Number: 1045, Message: "Access denied for user 'leitor'@'localhost' (using password: YES)"}, true},
		{"mysql 1044", &mysql.MySQLError{Number: 1044, Message: "Access denied for user 'sem_acesso'@'%' to database 'loja'"}, false},
		{"sem servidor", errors.New("dial tcp 127.0.0.1:1: connect: connection refused"), false},
	}
	for _, caso := range casos {
		w := httptest.NewRecorder()
		responderErroBanco(w, caso.err, bancos.Config{Host: "127.0.0.1", Porta: 1, Senha: "senha-de-teste"}, 30*time.Second)
		if w.Code != http.StatusUnprocessableEntity {
			t.Fatalf("%s: status %d", caso.nome, w.Code)
		}
		if strings.Contains(w.Body.String(), "senha-de-teste") {
			t.Fatalf("%s: a senha saiu na resposta: %s", caso.nome, w.Body.String())
		}
		var r map[string]any
		if err := json.Unmarshal(w.Body.Bytes(), &r); err != nil {
			t.Fatal(err)
		}
		if (r["senha_recusada"] == true) != caso.marcado {
			t.Fatalf("%s: senha_recusada=%v, esperava %v (%v)", caso.nome, r["senha_recusada"], caso.marcado, r)
		}
		if caso.marcado && r["erro"] != "Usuário ou senha recusados." {
			t.Fatalf("%s: frase %q", caso.nome, r["erro"])
		}
		if caso.nome == "mysql 1044" && r["erro"] != "Sem permissão no banco loja." {
			t.Fatalf("%s: frase %q", caso.nome, r["erro"])
		}
	}
}
