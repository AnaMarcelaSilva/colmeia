//go:build integracao

package bancos

// Testes com servidores de verdade, em contêineres próprios:
//
//	docker run -d --name colmeia-teste-mysql -p 127.0.0.1:33061:3306 -e MYSQL_ROOT_PASSWORD=teste-raiz -e MYSQL_DATABASE=loja mysql:8
//	docker run -d --name colmeia-teste-pg -p 127.0.0.1:54321:5432 -e POSTGRES_PASSWORD=teste-raiz -e POSTGRES_DB=loja postgres:16-alpine
//	COLMEIA_TESTE_MYSQL=127.0.0.1:33061 COLMEIA_TESTE_PG=127.0.0.1:54321 go test -tags integracao ./internal/bancos/
//	docker rm -f colmeia-teste-mysql colmeia-teste-pg

import (
	"context"
	"errors"
	"fmt"
	"net"
	"os"
	"strconv"
	"strings"
	"testing"
	"time"
)

const senhaDeTeste = "teste-raiz"

func servidor(t *testing.T, variavel, tipo, usuario string) Config {
	t.Helper()
	endereco := os.Getenv(variavel)
	if endereco == "" {
		t.Skip(variavel + " não definida")
	}
	host, porta, _ := net.SplitHostPort(endereco)
	p, _ := strconv.Atoi(porta)
	return Config{ID: 7, Tipo: tipo, Host: host, Porta: p, Usuario: usuario, Senha: senhaDeTeste, Banco: "loja", SSL: "desligado"}
}

func preparar(t *testing.T, g *Gerente, c Config, comandos ...string) {
	t.Helper()
	escrita := c
	escrita.Escrita = true
	escrita.ID = 99
	defer g.FecharConexao(99)
	for _, sql := range comandos {
		if _, err := g.Executar(context.Background(), escrita, Opcoes{SQL: sql, Confirmada: true, Tempo: time.Minute}); err != nil {
			t.Fatalf("%s: %v", sql, err)
		}
	}
}

func TestIntegracaoMySQL(t *testing.T) {
	c := servidor(t, "COLMEIA_TESTE_MYSQL", MySQL, "root")
	g := NovoGerente()
	defer g.FecharTodas()
	preparar(t, g, c,
		"DROP TABLE IF EXISTS pedidos", "DROP TABLE IF EXISTS clientes",
		"CREATE TABLE clientes (id INT PRIMARY KEY, nome VARCHAR(80))",
		"CREATE TABLE pedidos (id INT PRIMARY KEY AUTO_INCREMENT, cliente_id INT, total DECIMAL(10,2), FOREIGN KEY (cliente_id) REFERENCES clientes(id))",
		"INSERT INTO clientes VALUES (1, 'cliente-x')",
		"INSERT INTO pedidos (cliente_id, total) VALUES (1, 10.5), (1, 20)",
	)
	info, err := Testar(context.Background(), c)
	if err != nil || !strings.HasPrefix(info.Servidor, "MySQL 8") || info.TLS {
		t.Fatalf("testar: %+v %v", info, err)
	}
	// TLS em "exigir": o mysql:8 tem certificado próprio.
	comTLS := c
	comTLS.SSL = "exigir"
	if info, err := Testar(context.Background(), comTLS); err != nil || !info.TLS {
		t.Fatalf("testar com TLS: %+v %v", info, err)
	}
	verificar := c
	verificar.SSL = "verificar"
	if _, err := Testar(context.Background(), verificar); err == nil {
		t.Fatal("verificar sem a CA do servidor deveria falhar")
	} else if frase, _ := Explicar(err, verificar); frase != "O certificado não confere com a CA." {
		t.Fatalf("explicação: %q (%v)", frase, err)
	}
	errada := c
	errada.Senha = "errada-123"
	if _, err := Testar(context.Background(), errada); err == nil {
		t.Fatal("senha errada")
	} else if frase, detalhe := Explicar(err, errada); frase != "Usuário ou senha recusados." || !SenhaRecusada(err) || strings.Contains(detalhe, "errada-123") {
		t.Fatalf("senha errada: %q %q", frase, detalhe)
	}
	// Senha certa, mas sem acesso ao banco (1044): não é senha recusada.
	preparar(t, g, c, "DROP USER IF EXISTS 'sem_acesso'@'%'", "CREATE USER 'sem_acesso'@'%' IDENTIFIED BY 'certa-123'")
	defer preparar(t, g, c, "DROP USER IF EXISTS 'sem_acesso'@'%'")
	semAcesso := c
	semAcesso.Usuario, semAcesso.Senha = "sem_acesso", "certa-123"
	if _, err := Testar(context.Background(), semAcesso); err == nil {
		t.Fatal("sem acesso ao banco")
	} else if frase, _ := Explicar(err, semAcesso); frase != "Sem permissão no banco loja." || SenhaRecusada(err) {
		t.Fatalf("sem acesso: %q %v", frase, err)
	}
	r, err := g.Executar(context.Background(), c, Opcoes{SQL: "SELECT id, total FROM pedidos ORDER BY id"})
	if err != nil || len(r.Linhas) != 2 || *r.Linhas[0][1] != "10.50" || !r.Colunas[1].Numero {
		t.Fatalf("select: %+v %v", r, err)
	}
	// O classificador barra DDL (que no MySQL faria commit sozinho).
	if _, err := g.Executar(context.Background(), c, Opcoes{SQL: "DROP TABLE pedidos"}); !errors.Is(err, ErrSomenteLeitura) {
		t.Fatalf("drop: %v", err)
	}
	// Mesmo que passasse, a transação READ ONLY recusa o INSERT.
	db, devolver, _ := g.pegar(c, "")
	conn, _ := db.Conn(context.Background())
	conn.ExecContext(context.Background(), "START TRANSACTION READ ONLY")
	if _, err := conn.ExecContext(context.Background(), "INSERT INTO clientes VALUES (2, 'y')"); err == nil {
		t.Fatal("READ ONLY deveria recusar o INSERT")
	}
	conn.ExecContext(context.Background(), "ROLLBACK")
	conn.Close()
	devolver()
	// Cancelar SELECT SLEEP(30): KILL QUERY no servidor, em menos de 2 s.
	ficha := "ficha-mysql-sleep01"
	pronto := make(chan error)
	go func() {
		_, err := g.Executar(context.Background(), c, Opcoes{Ficha: ficha, SQL: "SELECT SLEEP(30)", Tempo: time.Minute})
		pronto <- err
	}()
	time.Sleep(500 * time.Millisecond)
	inicio := time.Now()
	g.Cancelar(ficha)
	if err := <-pronto; !errors.Is(err, ErrCancelada) || time.Since(inicio) > 2*time.Second {
		t.Fatalf("cancelar: %v em %v", err, time.Since(inicio))
	}
	time.Sleep(300 * time.Millisecond)
	r, _ = g.Executar(context.Background(), c, Opcoes{SQL: "SELECT COUNT(*) FROM information_schema.PROCESSLIST WHERE INFO LIKE 'SELECT SLEEP(30)%'"})
	if *r.Linhas[0][0] != "0" {
		t.Fatalf("a consulta continuou no servidor: %s", *r.Linhas[0][0])
	}
	// Árvore: bancos e tabelas (no MySQL, banco e esquema são o mesmo nível).
	bancos, err := g.Bancos(context.Background(), c)
	if err != nil || !contem(bancos, "loja") || contem(bancos, "mysql") {
		t.Fatalf("bancos: %v %v", bancos, err)
	}
	objetos, total, err := g.Objetos(context.Background(), c, "loja", "")
	if err != nil || total != 2 {
		t.Fatalf("objetos: %+v %v", objetos, err)
	}
	colunas, _ := g.Colunas(context.Background(), c, "loja", "", "pedidos")
	if len(colunas) != 3 || !colunas[0].PK || !colunas[1].FK {
		t.Fatalf("colunas: %+v", colunas)
	}
	if r, _, err := g.Previa(context.Background(), c, "loja", "", "pedidos", 100); err != nil || len(r.Linhas) != 2 {
		t.Fatalf("prévia: %v", err)
	}
}

func contem(lista []string, nome string) bool {
	for _, n := range lista {
		if n == nome {
			return true
		}
	}
	return false
}

func TestIntegracaoPostgres(t *testing.T) {
	c := servidor(t, "COLMEIA_TESTE_PG", Postgres, "postgres")
	// O ambiente do usuário não pode mudar nada.
	t.Setenv("PGPASSWORD", "falsa")
	t.Setenv("PGHOST", "203.0.113.1")
	t.Setenv("PGPORT", "1")
	t.Setenv("PGDATABASE", "nao_existe")
	t.Setenv("PGOPTIONS", "-c default_transaction_read_only=off")
	t.Setenv("PGSSLMODE", "verify-full")
	g := NovoGerente()
	defer g.FecharTodas()
	var tabelas []string
	for i := 1; i <= 1007; i++ {
		tabelas = append(tabelas, fmt.Sprintf("CREATE TABLE muitas.t%04d (id int)", i))
	}
	preparar(t, g, c, append([]string{
		"DROP SCHEMA IF EXISTS muitas CASCADE", "CREATE SCHEMA muitas",
		"DROP TABLE IF EXISTS pedidos CASCADE", "DROP TABLE IF EXISTS clientes CASCADE",
		"CREATE TABLE clientes (id int PRIMARY KEY, nome text)",
		"CREATE TABLE pedidos (id serial PRIMARY KEY, cliente_id int REFERENCES clientes(id), total numeric(10,2), dados jsonb, codigo uuid)",
		"INSERT INTO clientes VALUES (1, 'cliente-x')",
		"INSERT INTO pedidos (cliente_id, total, dados, codigo) VALUES (1, 10.5, '{\"a\": 1}', '01020304-0506-0708-090a-0b0c0d0e0f10')",
		"CREATE OR REPLACE VIEW grandes AS SELECT * FROM pedidos",
	}, tabelas...)...)
	if !temBanco(t, g, c, "outro") {
		preparar(t, g, c, "CREATE DATABASE outro")
	}
	info, err := Testar(context.Background(), c)
	if err != nil || !strings.HasPrefix(info.Servidor, "PostgreSQL 16") {
		t.Fatalf("testar: %+v %v", info, err)
	}
	r, err := g.Executar(context.Background(), c, Opcoes{SQL: "SELECT total, dados, codigo, current_setting('default_transaction_read_only') FROM pedidos"})
	if err != nil || *r.Linhas[0][0] != "10.50" || *r.Linhas[0][1] != `{"a": 1}` || *r.Linhas[0][2] != "01020304-0506-0708-090a-0b0c0d0e0f10" || *r.Linhas[0][3] != "on" {
		t.Fatalf("select: %v %v", r.Linhas, err)
	}
	// A transação só de leitura barra o que passasse pelo classificador.
	db, devolver, _ := g.pegar(c, "")
	if _, err := db.Exec("INSERT INTO clientes VALUES (2, 'y')"); err == nil {
		t.Fatal("default_transaction_read_only deveria barrar")
	}
	devolver()
	// pg_sleep cancelado e statement_timeout.
	ficha := "ficha-pg-sleep-0001"
	pronto := make(chan error)
	go func() {
		_, err := g.Executar(context.Background(), c, Opcoes{Ficha: ficha, SQL: "SELECT pg_sleep(30)", Tempo: time.Minute})
		pronto <- err
	}()
	time.Sleep(500 * time.Millisecond)
	inicio := time.Now()
	g.Cancelar(ficha)
	if err := <-pronto; !errors.Is(err, ErrCancelada) || time.Since(inicio) > 2*time.Second {
		t.Fatalf("cancelar: %v", err)
	}
	if _, err := g.Executar(context.Background(), c, Opcoes{SQL: "SELECT pg_sleep(5)", Tempo: 300 * time.Millisecond}); !errors.Is(err, ErrTempoEsgotado) {
		t.Fatalf("tempo-limite: %v", err)
	}
	// Erro com a linha.
	_, err = g.Executar(context.Background(), c, Opcoes{SQL: "SELECT 1\nFROM\nnao_existe"})
	var eb ErrBanco
	if !errors.As(err, &eb) || eb.Linha != 3 {
		t.Fatalf("erro com linha: %#v", err)
	}
	bancos, err := g.Bancos(context.Background(), c)
	if err != nil || !contem(bancos, "loja") || !contem(bancos, "outro") || contem(bancos, "template0") {
		t.Fatalf("bancos: %v %v", bancos, err)
	}
	esquemas, _ := g.Esquemas(context.Background(), c, "loja")
	if !contem(esquemas, "public") || !contem(esquemas, "muitas") || contem(esquemas, "pg_catalog") {
		t.Fatalf("esquemas: %v", esquemas)
	}
	objetos, total, err := g.Objetos(context.Background(), c, "loja", "muitas")
	if err != nil || total != 1007 || len(objetos) != 1007 {
		t.Fatalf("1007 tabelas: %d %v", total, err)
	}
	objetos, _, _ = g.Objetos(context.Background(), c, "loja", "public")
	if len(objetos) != 3 || !objetos[1].View || objetos[0].View {
		t.Fatalf("public: %+v", objetos)
	}
	colunas, _ := g.Colunas(context.Background(), c, "loja", "public", "pedidos")
	if len(colunas) != 5 || !colunas[0].PK || !colunas[1].FK || colunas[2].Tipo != "numeric(10,2)" {
		t.Fatalf("colunas: %+v", colunas)
	}
	// Outro banco do mesmo servidor, num pool próprio.
	if _, err := g.Esquemas(context.Background(), c, "outro"); err != nil {
		t.Fatal(err)
	}
	if pools, _ := g.Abertos(); pools < 2 {
		t.Fatalf("pools: %d", pools)
	}
	exigir := c
	exigir.SSL = "exigir"
	if _, err := Testar(context.Background(), exigir); err == nil {
		t.Fatal("o postgres:16-alpine não tem TLS: exigir deveria falhar")
	}
	preferir := c
	preferir.SSL = "preferir"
	if info, err := Testar(context.Background(), preferir); err != nil || info.TLS {
		t.Fatalf("preferir sem TLS no servidor cai para texto puro: %+v %v", info, err)
	}
}

func temBanco(t *testing.T, g *Gerente, c Config, nome string) bool {
	bancos, err := g.Bancos(context.Background(), c)
	if err != nil {
		t.Fatal(err)
	}
	return contem(bancos, nome)
}
