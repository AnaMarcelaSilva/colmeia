package bancos

import (
	"context"
	"database/sql"
	"errors"
	"fmt"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// lojaDeTeste cria um SQLite com pedidos e clientes (n pedidos).
func lojaDeTeste(t *testing.T, n int) string {
	t.Helper()
	arquivo := filepath.Join(t.TempDir(), "loja.db")
	db, err := sql.Open("sqlite", arquivo)
	if err != nil {
		t.Fatal(err)
	}
	defer db.Close()
	for _, c := range []string{
		`CREATE TABLE clientes (id INTEGER PRIMARY KEY, nome TEXT NOT NULL, foto BLOB)`,
		`CREATE TABLE pedidos (id INTEGER PRIMARY KEY, cliente_id INTEGER REFERENCES clientes(id), total REAL, criado_em TEXT, obs TEXT)`,
		`CREATE VIEW grandes AS SELECT * FROM pedidos WHERE total > 100`,
		`INSERT INTO clientes (nome, foto) VALUES ('cliente-x', x'00ff10'), ('cliente-y', NULL)`,
	} {
		if _, err := db.Exec(c); err != nil {
			t.Fatal(c, err)
		}
	}
	tx, _ := db.Begin()
	for i := 1; i <= n; i++ {
		tx.Exec(`INSERT INTO pedidos (cliente_id, total, criado_em, obs) VALUES (?, ?, '2026-10-01', ?)`, 1+i%2, float64(i)*1.5, nil)
	}
	tx.Commit()
	return arquivo
}

func configSQLite(arquivo string, escrita bool) Config {
	return Config{ID: 1, Tipo: SQLite, Arquivo: arquivo, Escrita: escrita}
}

func TestExecutarLeitura(t *testing.T) {
	g := NovoGerente()
	defer g.FecharTodas()
	c := configSQLite(lojaDeTeste(t, 10), false)
	r, err := g.Executar(context.Background(), c, Opcoes{SQL: "SELECT id, nome, foto FROM clientes ORDER BY id", Limite: 500})
	if err != nil {
		t.Fatal(err)
	}
	if len(r.Linhas) != 2 || r.Mais || r.Verbo != "SELECT" || len(r.Colunas) != 3 {
		t.Fatalf("resultado: %+v", r)
	}
	if *r.Linhas[0][1] != "cliente-x" || *r.Linhas[0][2] != "<binário 3 bytes>" || r.Linhas[1][2] != nil {
		t.Fatalf("células: %v %v %v", *r.Linhas[0][1], *r.Linhas[0][2], r.Linhas[1][2])
	}
	if !r.Colunas[0].Numero || r.Colunas[1].Numero {
		t.Fatalf("colunas numéricas: %+v", r.Colunas)
	}
}

func TestSomenteLeituraEConfirmacao(t *testing.T) {
	g := NovoGerente()
	defer g.FecharTodas()
	arquivo := lojaDeTeste(t, 3)
	leitura := configSQLite(arquivo, false)
	if _, err := g.Executar(context.Background(), leitura, Opcoes{SQL: "INSERT INTO clientes (nome) VALUES ('z')"}); !errors.Is(err, ErrSomenteLeitura) {
		t.Fatalf("insert em leitura: %v", err)
	}
	// Mesmo que o classificador errasse, o arquivo está aberto só para leitura.
	db, devolver, err := g.pegar(leitura, "")
	if err != nil {
		t.Fatal(err)
	}
	if _, err := db.Exec("INSERT INTO clientes (nome) VALUES ('z')"); err == nil {
		t.Fatal("mode=ro deveria recusar a escrita")
	}
	devolver()
	g.FecharConexao(1)
	escrita := configSQLite(arquivo, true)
	_, err = g.Executar(context.Background(), escrita, Opcoes{SQL: "UPDATE pedidos SET total = 0"})
	var confirmar ErrPrecisaConfirmar
	if !errors.As(err, &confirmar) || confirmar.Verbo != "UPDATE" || !confirmar.SemWhere {
		t.Fatalf("sem confirmar: %v", err)
	}
	r, err := g.Executar(context.Background(), escrita, Opcoes{SQL: "UPDATE pedidos SET total = 0", Confirmada: true})
	if err != nil || r.Afetadas == nil || *r.Afetadas != 3 || !r.Altera {
		t.Fatalf("update confirmado: %+v %v", r, err)
	}
	r, _ = g.Executar(context.Background(), escrita, Opcoes{SQL: "SELECT SUM(total) FROM pedidos"})
	if *r.Linhas[0][0] != "0" {
		t.Fatalf("a alteração foi gravada: %v", *r.Linhas[0][0])
	}
	// Uma leitura numa conexão com escrita é desfeita no fim (nada a gravar).
	if _, err := g.Executar(context.Background(), escrita, Opcoes{SQL: "SELECT 1; SELECT 2"}); !errors.Is(err, ErrVarias) {
		t.Fatalf("duas instruções: %v", err)
	}
}

func TestCarregarMaisECursorParado(t *testing.T) {
	g := NovoGerente()
	g.Tempos.CursorParado = 150 * time.Millisecond
	defer g.FecharTodas()
	c := configSQLite(lojaDeTeste(t, 1200), false)
	ficha := "ficha-de-teste-0001"
	r, err := g.Executar(context.Background(), c, Opcoes{Ficha: ficha, SQL: "SELECT * FROM pedidos ORDER BY id", Limite: 500})
	if err != nil || len(r.Linhas) != 500 || !r.Mais {
		t.Fatalf("primeira página: %d %v %v", len(r.Linhas), r.Mais, err)
	}
	r, err = g.Mais(context.Background(), ficha, 500)
	if err != nil || len(r.Linhas) != 500 || !r.Mais || *r.Linhas[0][0] != "501" {
		t.Fatalf("segunda página: %d %v %v", len(r.Linhas), r.Mais, err)
	}
	r, err = g.Mais(context.Background(), ficha, 500)
	if err != nil || len(r.Linhas) != 200 || r.Mais {
		t.Fatalf("última página: %d %v %v", len(r.Linhas), r.Mais, err)
	}
	if _, err := g.Mais(context.Background(), ficha, 500); !errors.Is(err, ErrSemExecucao) {
		t.Fatalf("depois do fim: %v", err)
	}
	// Exatamente o limite: sem "há mais".
	r, _ = g.Executar(context.Background(), c, Opcoes{SQL: "SELECT id FROM pedidos LIMIT 500", Limite: 500})
	if r.Mais {
		t.Fatal("500 de 500 não tem mais")
	}
	// Parada demais, a execução fecha sozinha.
	if _, err := g.Executar(context.Background(), c, Opcoes{Ficha: ficha, SQL: "SELECT * FROM pedidos", Limite: 10}); err != nil {
		t.Fatal(err)
	}
	time.Sleep(400 * time.Millisecond)
	if _, err := g.Mais(context.Background(), ficha, 10); !errors.Is(err, ErrSemExecucao) {
		t.Fatalf("depois de parada: %v", err)
	}
	if _, execucoes := g.Abertos(); execucoes != 0 {
		t.Fatalf("execuções abertas: %d", execucoes)
	}
}

func TestUmCursorPorConexao(t *testing.T) {
	g := NovoGerente()
	defer g.FecharTodas()
	c := configSQLite(lojaDeTeste(t, 50), false)
	g.Executar(context.Background(), c, Opcoes{Ficha: "primeira-ficha-0001", SQL: "SELECT * FROM pedidos", Limite: 10})
	g.Executar(context.Background(), c, Opcoes{Ficha: "segunda-ficha-00001", SQL: "SELECT * FROM pedidos", Limite: 10})
	if _, err := g.Mais(context.Background(), "primeira-ficha-0001", 10); !errors.Is(err, ErrSemExecucao) {
		t.Fatalf("a anterior deveria fechar: %v", err)
	}
	if _, err := g.Mais(context.Background(), "segunda-ficha-00001", 10); err != nil {
		t.Fatal(err)
	}
}

const consultaPesada = `WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 500000000) SELECT count(*) FROM n`

func TestTempoLimiteECancelar(t *testing.T) {
	g := NovoGerente()
	defer g.FecharTodas()
	c := configSQLite(lojaDeTeste(t, 1), false)
	inicio := time.Now()
	_, err := g.Executar(context.Background(), c, Opcoes{SQL: consultaPesada, Tempo: 200 * time.Millisecond})
	if !errors.Is(err, ErrTempoEsgotado) || time.Since(inicio) > 3*time.Second {
		t.Fatalf("tempo-limite: %v em %v", err, time.Since(inicio))
	}
	ficha := "ficha-para-cancelar"
	pronto := make(chan error)
	go func() {
		_, err := g.Executar(context.Background(), c, Opcoes{Ficha: ficha, SQL: consultaPesada, Tempo: time.Minute})
		pronto <- err
	}()
	time.Sleep(200 * time.Millisecond)
	if !g.Cancelar(ficha) {
		t.Fatal("a execução deveria estar rodando")
	}
	select {
	case err := <-pronto:
		if !errors.Is(err, ErrCancelada) {
			t.Fatalf("cancelada: %v", err)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("o cancelamento não parou a consulta")
	}
	// O pedido de quem espera acabou (a tela fechou): para também.
	ctx, cancelar := context.WithTimeout(context.Background(), 150*time.Millisecond)
	defer cancelar()
	if _, err := g.Executar(ctx, c, Opcoes{SQL: consultaPesada, Tempo: time.Minute}); !errors.Is(err, ErrCancelada) {
		t.Fatalf("pedido desistiu: %v", err)
	}
}

func TestArvoreEPreviaSQLite(t *testing.T) {
	g := NovoGerente()
	defer g.FecharTodas()
	c := configSQLite(lojaDeTeste(t, 150), false)
	objetos, total, err := g.Objetos(context.Background(), c, "", "")
	if err != nil || total != 3 || len(objetos) != 3 {
		t.Fatalf("objetos: %+v %d %v", objetos, total, err)
	}
	if objetos[1].Nome != "grandes" || !objetos[1].View || objetos[2].Nome != "pedidos" || objetos[2].View {
		t.Fatalf("ordem e views: %+v", objetos)
	}
	colunas, err := g.Colunas(context.Background(), c, "", "", "pedidos")
	if err != nil || len(colunas) != 5 || !colunas[0].PK || !colunas[1].FK || colunas[2].Tipo != "REAL" {
		t.Fatalf("colunas: %+v %v", colunas, err)
	}
	r, sqlPrevia, err := g.Previa(context.Background(), c, "", "", "pedidos", 100)
	if err != nil || len(r.Linhas) != 100 || sqlPrevia != `SELECT * FROM "pedidos" LIMIT 100` {
		t.Fatalf("prévia: %d %q %v", len(r.Linhas), sqlPrevia, err)
	}
	if _, _, err := g.Previa(context.Background(), c, "", "", `pedidos" ; DROP TABLE x; --`, 100); err == nil {
		t.Fatal("prévia de tabela que não existe")
	}
}

func TestCelula(t *testing.T) {
	casos := []struct {
		v     any
		tipo  string
		texto string
	}{
		{[]byte("abc"), "varchar", "abc"},
		{[]byte{0xff, 0x00}, "", "<binário 2 bytes>"},
		{[]byte{0x01, 0x02}, "", "<binário 2 bytes>"},
		{[]byte("a\tb\nc"), "", "a\tb\nc"},
		{[]byte("a\x01b"), "text", "a\x01b"},
		{make([]byte, 3000), "bytea", "<binário 3 KB>"},
		{int64(7), "", "7"},
		{1.5, "", "1.5"},
		{true, "", "true"},
		{time.Date(2026, 10, 2, 0, 0, 0, 0, time.UTC), "date", "2026-10-02"},
		{time.Date(2026, 10, 2, 14, 5, 6, 0, time.UTC), "timestamp", "2026-10-02 14:05:06"},
		{[16]byte{1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16}, "uuid", "01020304-0506-0708-090a-0b0c0d0e0f10"},
		{map[string]any{"a": 1}, "json", `{"a":1}`},
	}
	for _, c := range casos {
		if got := Celula(c.v, c.tipo); got == nil || *got != c.texto {
			t.Errorf("%v: %v", c.v, got)
		}
	}
	if Celula(nil, "") != nil {
		t.Error("nil é NULL")
	}
	longo := strings.Repeat("é", MaxCelula)
	if got := *Celula(longo, "text"); len(got) > MaxCelula+4 || !strings.HasSuffix(got, "…") {
		t.Errorf("corte: %d", len(got))
	}
}

func TestPoolOciosoFecha(t *testing.T) {
	g := NovoGerente()
	g.Tempos.Ocioso = 100 * time.Millisecond
	defer g.FecharTodas()
	c := configSQLite(lojaDeTeste(t, 1), false)
	g.Executar(context.Background(), c, Opcoes{SQL: "SELECT 1"})
	if pools, _ := g.Abertos(); pools != 1 {
		t.Fatalf("pools: %d", pools)
	}
	time.Sleep(400 * time.Millisecond)
	if pools, _ := g.Abertos(); pools != 0 {
		t.Fatalf("o pool ocioso deveria fechar: %d", pools)
	}
}

func TestErroComLinhaERedigido(t *testing.T) {
	err := erroDoBanco(fmt.Errorf("falhou com a senha canario-7f3a-SENHA"), "canario-7f3a-SENHA", "")
	if strings.Contains(err.Error(), "canario") {
		t.Fatal("a senha vazou no erro")
	}
	frase, detalhe := Explicar(fmt.Errorf("dial tcp 127.0.0.1:1: connect: connection refused"), Config{Host: "127.0.0.1", Porta: 1})
	if frase != "Não achou o servidor 127.0.0.1:1." || detalhe == "" {
		t.Fatalf("explicação: %q", frase)
	}
}

func TestSQLPreviaECitar(t *testing.T) {
	if got := SQLPrevia(MySQL, "lo`ja", "", "pedidos", 100); got != "SELECT * FROM `lo``ja`.`pedidos` LIMIT 100" {
		t.Error(got)
	}
	if got := SQLPrevia(SQLServer, "loja", "dbo", "pe]didos", 100); got != "SELECT TOP 100 * FROM [loja].[dbo].[pe]]didos]" {
		t.Error(got)
	}
	if got := SQLPrevia(Postgres, "loja", "public", `a"b`, 100); got != `SELECT * FROM "public"."a""b" LIMIT 100` {
		t.Error(got)
	}
	if got := SQLGerado(Postgres, "public", "pedidos", 100); got != "SELECT * FROM public.pedidos LIMIT 100;" {
		t.Error(got)
	}
	if got := SQLGerado(SQLServer, "dbo", "Pedidos", 100); got != "SELECT TOP 100 * FROM dbo.[Pedidos];" {
		t.Error(got)
	}
}
