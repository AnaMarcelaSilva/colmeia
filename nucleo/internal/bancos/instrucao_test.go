package bancos

import (
	"errors"
	"testing"
)

func TestClassificar(t *testing.T) {
	casos := []struct {
		dialeto, sql, classe, verbo string
		semWhere                    bool
	}{
		{Postgres, "select * from pedidos", Leitura, "SELECT", false},
		{Postgres, "  -- comentário\n SELECT 1;", Leitura, "SELECT", false},
		{Postgres, "(select 1) union (select 2)", Leitura, "SELECT", false},
		{Postgres, "SELECT 'drop table x; delete' AS t", Leitura, "SELECT", false},
		{Postgres, `SELECT "update" FROM "insert"`, Leitura, "SELECT", false},
		{Postgres, "SELECT $$ delete from x; $$", Leitura, "SELECT", false},
		{Postgres, "SELECT $tag$ ; drop $tag$", Leitura, "SELECT", false},
		{Postgres, "SELECT $1", Leitura, "SELECT", false},
		{Postgres, "WITH x AS (DELETE FROM pedidos RETURNING *) SELECT * FROM x", Altera, "DELETE", false},
		{MySQL, "WITH x AS (SELECT 1) SELECT * FROM x", Leitura, "WITH", false},
		{Postgres, "SELECT * INTO nova FROM pedidos", Altera, "SELECT", false},
		{MySQL, "SELECT * FROM pedidos INTO OUTFILE '/tmp/x'", Altera, "SELECT", false},
		{Postgres, "SELECT * FROM pedidos FOR UPDATE", Altera, "UPDATE", false},
		{Postgres, "SELECT * FROM pedidos FOR SHARE", Altera, "SELECT", false},
		{MySQL, "SELECT * FROM pedidos LOCK IN SHARE MODE", Altera, "LOCK", false},
		{Postgres, "EXPLAIN SELECT 1", Leitura, "EXPLAIN", false},
		{Postgres, "EXPLAIN ANALYZE DELETE FROM pedidos", Altera, "EXPLAIN", false},
		{Postgres, "EXPLAIN (ANALYZE) SELECT 1", Altera, "EXPLAIN", false},
		{MySQL, "SHOW TABLES", Leitura, "SHOW", false},
		{MySQL, "DESCRIBE pedidos", Leitura, "DESCRIBE", false},
		{MySQL, "SELECT REPLACE(nome, 'a', 'b') FROM clientes", Leitura, "SELECT", false},
		{MySQL, "REPLACE INTO clientes VALUES (1)", Altera, "REPLACE", false},
		{MySQL, "SELECT 1 /*!50000 , (DELETE FROM x) */", Altera, "DELETE", false},
		{MySQL, "SELECT 1 # ; drop table x", Leitura, "SELECT", false},
		{MySQL, "SELECT 'a\\'; drop table x'", Leitura, "SELECT", false},
		{MySQL, `SELECT "texto; delete"`, Leitura, "SELECT", false},
		{MySQL, "SELECT `delete` FROM `update`", Leitura, "SELECT", false},
		{SQLServer, "SELECT [delete] FROM [dbo].[pedidos]", Leitura, "SELECT", false},
		{SQLServer, "SELECT TOP 10 * FROM pedidos /* /* drop */ delete */", Leitura, "SELECT", false},
		{SQLServer, "EXEC sp_who", Altera, "EXEC", false},
		{SQLServer, "SELECT * FROM OPENROWSET('x','y','z')", Altera, "OPENROWSET", false},
		{SQLite, "PRAGMA table_info(pedidos)", Leitura, "PRAGMA", false},
		{SQLite, "PRAGMA journal_mode = WAL", Altera, "PRAGMA", false},
		{SQLite, "PRAGMA user_version(5)", Altera, "PRAGMA", false},
		{Postgres, "PRAGMA table_info(x)", Altera, "PRAGMA", false},
		{Postgres, "UPDATE pedidos SET total = 0", Altera, "UPDATE", true},
		{Postgres, "update pedidos set total = 0 where id = 1", Altera, "UPDATE", false},
		{Postgres, "DELETE FROM pedidos", Altera, "DELETE", true},
		{Postgres, "TRUNCATE pedidos", Altera, "TRUNCATE", false},
		{Postgres, "CREATE TABLE x (id int)", Altera, "CREATE", false},
		{Postgres, "SET search_path = x", Altera, "SET", false},
		{Postgres, "DO $$ BEGIN END $$", Altera, "DO", false},
		{Postgres, "VACUUM", Altera, "VACUUM", false},
		{Postgres, "BEGIN", Altera, "BEGIN", false},
		{Postgres, "VALUES (1), (2)", Leitura, "VALUES", false},
		{Postgres, "TABLE pedidos", Leitura, "TABLE", false},
		{Postgres, "; ; SELECT 1 ;", Leitura, "SELECT", false},
		{Postgres, "'texto solto'", Altera, "?", false},
	}
	for _, c := range casos {
		got, err := Classificar(c.dialeto, c.sql)
		if err != nil {
			t.Errorf("%s %q: erro %v", c.dialeto, c.sql, err)
			continue
		}
		if got.Classe != c.classe || got.Verbo != c.verbo || got.SemWhere != c.semWhere {
			t.Errorf("%s %q: %+v, esperava %s %s semWhere=%v", c.dialeto, c.sql, got, c.classe, c.verbo, c.semWhere)
		}
	}
}

func TestClassificarVariasOuVazia(t *testing.T) {
	if _, err := Classificar(Postgres, "SELECT 1; SELECT 2"); !errors.Is(err, ErrVarias) {
		t.Errorf("duas instruções: %v", err)
	}
	if _, err := Classificar(Postgres, "SELECT 1; DROP TABLE x"); !errors.Is(err, ErrVarias) {
		t.Errorf("leitura seguida de alteração: %v", err)
	}
	if _, err := Classificar(Postgres, "  -- só comentário\n /* e outro */ ;"); !errors.Is(err, ErrVazia) {
		t.Errorf("vazia: %v", err)
	}
	// O ; dentro de texto, citação e comentário não divide.
	if _, err := Classificar(MySQL, "SELECT ';' , `;` -- ;\n FROM x"); err != nil {
		t.Errorf("; protegido: %v", err)
	}
}

func TestDividirESobCursor(t *testing.T) {
	sql := "SELECT 1;\n\n-- dois\nSELECT 'a;b' ;\nUPDATE x SET y = 1"
	trechos := Dividir(Postgres, sql)
	if len(trechos) != 3 {
		t.Fatalf("trechos: %+v", trechos)
	}
	textos := []string{"SELECT 1", "SELECT 'a;b'", "UPDATE x SET y = 1"}
	for i, tr := range trechos {
		if sql[tr.Ini:tr.Fim] != textos[i] {
			t.Errorf("trecho %d: %q", i, sql[tr.Ini:tr.Fim])
		}
	}
	casos := []struct {
		pos   int
		texto string
	}{
		{0, "SELECT 1"},
		{8, "SELECT 1"},
		{10, "SELECT 1"},                        // na linha vazia: a anterior
		{len("SELECT 1;\n\n-- do"), "SELECT 1"}, // no comentário antes da segunda
		{len(sql), "UPDATE x SET y = 1"},
		{len("SELECT 1;\n\n-- dois\nSELECT 'a"), "SELECT 'a;b'"},
	}
	for _, c := range casos {
		tr, ok := SobCursor(Postgres, sql, c.pos)
		if !ok || sql[tr.Ini:tr.Fim] != c.texto {
			t.Errorf("cursor %d: %q", c.pos, sql[tr.Ini:tr.Fim])
		}
	}
	if _, ok := SobCursor(Postgres, "  -- nada", 3); ok {
		t.Error("sem instrução não escolhe nada")
	}
	// Antes da primeira: a primeira.
	if tr, _ := SobCursor(Postgres, "\n\nSELECT 2", 0); tr.Ini != 2 {
		t.Errorf("antes da primeira: %+v", tr)
	}
}

func TestPedacosTextoSemFim(t *testing.T) {
	// Texto aberto vai até o fim, sem pânico, e não esconde nada depois.
	for _, d := range []string{Postgres, MySQL, SQLServer, SQLite} {
		for _, sql := range []string{"SELECT 'abc", "SELECT \"abc", "SELECT /* abc", "SELECT $x$ abc", "SELECT [abc", "SELECT `abc", "é ç ã", "$", "--", "/*"} {
			p := Pedacos(d, sql)
			if len(p) == 0 || p[len(p)-1].Fim != len(sql) {
				t.Errorf("%s %q: %+v", d, sql, p)
			}
		}
	}
}

func TestFuncaoNegadaAoAgente(t *testing.T) {
	casos := map[string]string{
		"SELECT pg_terminate_backend(123)":         "pg_terminate_backend",
		"SELECT * FROM dblink_exec('x')":           "dblink_exec",
		"SELECT lo_import('/etc/passwd')":          "lo_import",
		"select set_config('x','y',false)":         "set_config",
		"SELECT LOAD_FILE('/etc/passwd')":          "load_file",
		"SELECT 'pg_terminate_backend' FROM x":     "",
		"SELECT nome FROM clientes":                "",
		`SELECT "pg_read_file" FROM x`:             "",
		"SELECT pg_read_file('/etc/hostname')":     "pg_read_file",
		"SELECT * FROM pg_ls_dir('.')":             "pg_ls_dir",
		"SELECT count(*) FROM pedidos WHERE 1 = 1": "",
	}
	for sql, esperado := range casos {
		if got := FuncaoNegadaAoAgente(Postgres, sql); got != esperado {
			t.Errorf("%q: %q, esperava %q", sql, got, esperado)
		}
	}
}
