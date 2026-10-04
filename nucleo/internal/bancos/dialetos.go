package bancos

import (
	"context"
	"crypto/tls"
	"crypto/x509"
	"database/sql"
	"errors"
	"fmt"
	"net"
	"net/url"
	"os"
	"strconv"
	"strings"
	"time"

	"github.com/go-sql-driver/mysql"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/stdlib"
	mssql "github.com/microsoft/go-mssqldb"
	_ "modernc.org/sqlite"
)

// Config é o que o núcleo precisa para abrir uma conexão (já com a senha,
// que só passa por aqui em memória).
type Config struct {
	// ID da conexão: chave dos pools e das execuções.
	ID      int64
	Tipo    string
	Host    string
	Porta   int
	Usuario string
	Senha   string
	// Banco padrão da conexão (vazio: todos).
	Banco   string
	Arquivo string
	SSL     string
	SSLCA   string
	Escrita bool
}

// TempoParaConectar ao servidor.
const TempoParaConectar = 10 * time.Second

// Os drivers não escrevem nada no log do núcleo.
type semLog struct{}

func (semLog) Print(...any) {}

// configTLS monta o TLS da conexão: Preferir e Exigir cifram sem conferir o
// certificado (como o sslmode=require do libpq); Verificar confere com a CA
// indicada (ou as do sistema) e o nome do servidor.
func configTLS(c Config) (*tls.Config, error) {
	switch c.SSL {
	case "preferir", "exigir":
		return &tls.Config{InsecureSkipVerify: true, MinVersion: tls.VersionTLS12}, nil
	case "verificar":
		t := &tls.Config{ServerName: c.Host, MinVersion: tls.VersionTLS12}
		if c.SSLCA != "" {
			pem, err := os.ReadFile(c.SSLCA)
			if err != nil {
				return nil, fmt.Errorf("não consegui ler o certificado da CA: %w", err)
			}
			raizes := x509.NewCertPool()
			if !raizes.AppendCertsFromPEM(pem) {
				return nil, errors.New("o arquivo da CA não tem um certificado PEM")
			}
			t.RootCAs = raizes
		}
		return t, nil
	}
	return nil, nil
}

// citarPG escapa um valor do formato chave='valor' do Postgres.
func citarPG(v string) string {
	return "'" + strings.NewReplacer(`\`, `\\`, `'`, `\'`).Replace(v) + "'"
}

// configPostgres monta a configuração do pgx sem deixar o ambiente do
// usuário (PGHOST, PGPASSWORD, ~/.pgpass, PGOPTIONS, certificados de
// cliente...) influenciar: tudo o que importa é fixado depois de lido.
func configPostgres(c Config, banco string) (*pgx.ConnConfig, error) {
	if banco == "" {
		banco = c.Banco
	}
	if banco == "" {
		banco = "postgres"
	}
	partes := []string{
		"host=" + citarPG(c.Host), "port=" + strconv.Itoa(c.Porta), "dbname=" + citarPG(banco), "user=" + citarPG(c.Usuario),
		"sslmode=disable", "connect_timeout=" + strconv.Itoa(int(TempoParaConectar.Seconds())), "target_session_attrs=any",
		"sslnegotiation=postgres", "passfile=" + citarPG("/nonexistent/colmeia-sem-pgpass"),
	}
	config, err := pgx.ParseConfigWithOptions(strings.Join(partes, " "), pgx.ParseConfigOptions{ParseConfigOptions: pgconn.ParseConfigOptions{
		ConnStringAllowedKeys: []string{"host", "port", "dbname", "user", "sslmode", "connect_timeout", "target_session_attrs", "sslnegotiation", "passfile"},
	}})
	if err != nil {
		return nil, errors.New("configuração do Postgres recusada (confira as variáveis PGSERVICE e PGSERVICEFILE do ambiente)")
	}
	config.Host, config.Port, config.Database, config.User, config.Password = c.Host, uint16(c.Porta), banco, c.Usuario, c.Senha
	config.KerberosSrvName, config.KerberosSpn = "", ""
	config.ValidateConnect, config.AfterConnect, config.OnNotice, config.OnNotification = nil, nil, nil, nil
	config.MinProtocolVersion, config.MaxProtocolVersion, config.ChannelBinding, config.RequireAuth = "", "", "prefer", ""
	config.SSLNegotiation = "postgres"
	config.OAuthTokenProvider = nil
	config.RuntimeParams = map[string]string{"application_name": "Colmeia"}
	if !c.Escrita {
		config.RuntimeParams["default_transaction_read_only"] = "on"
	}
	config.DefaultQueryExecMode = pgx.QueryExecModeExec
	config.StatementCacheCapacity, config.DescriptionCacheCapacity = 0, 0
	config.Tracer = nil
	t, err := configTLS(c)
	if err != nil {
		return nil, err
	}
	config.TLSConfig, config.Fallbacks = t, nil
	if c.SSL == "preferir" {
		// Tenta com TLS e cai para texto puro (o teste e a barra mostram se usou TLS).
		config.Fallbacks = []*pgconn.FallbackConfig{{Host: c.Host, Port: uint16(c.Porta), TLSConfig: nil}}
	}
	return config, nil
}

func configMySQL(c Config, banco string) (*mysql.Config, error) {
	cfg := mysql.NewConfig()
	cfg.Net = "tcp"
	cfg.Addr = net.JoinHostPort(c.Host, strconv.Itoa(c.Porta))
	cfg.User, cfg.Passwd = c.Usuario, c.Senha
	cfg.DBName = banco
	if banco == "" {
		cfg.DBName = c.Banco
	}
	// LOAD DATA LOCAL INFILE desligado: um servidor malicioso não pede arquivos daqui.
	cfg.AllowAllFiles = false
	cfg.MultiStatements = false
	cfg.InterpolateParams = false
	cfg.AllowCleartextPasswords = false
	cfg.AllowOldPasswords = false
	cfg.Timeout = TempoParaConectar
	cfg.Logger = semLog{}
	cfg.Params = map[string]string{}
	switch c.SSL {
	case "desligado":
		cfg.TLSConfig = "false"
	case "preferir":
		cfg.TLSConfig = "preferred"
	case "exigir":
		cfg.TLSConfig = "skip-verify"
	case "verificar":
		t, err := configTLS(c)
		if err != nil {
			return nil, err
		}
		cfg.TLS = t
	}
	return cfg, nil
}

func dsnSQLServer(c Config, banco string) (string, error) {
	u := &url.URL{Scheme: "sqlserver", User: url.UserPassword(c.Usuario, c.Senha), Host: net.JoinHostPort(c.Host, strconv.Itoa(c.Porta))}
	q := url.Values{}
	if banco == "" {
		banco = c.Banco
	}
	if banco != "" {
		q.Set("database", banco)
	}
	q.Set("app name", "Colmeia")
	q.Set("connection timeout", strconv.Itoa(int(TempoParaConectar.Seconds())))
	switch c.SSL {
	case "desligado":
		q.Set("encrypt", "disable")
	case "preferir":
		q.Set("encrypt", "false")
		q.Set("TrustServerCertificate", "true")
	case "exigir":
		q.Set("encrypt", "true")
		q.Set("TrustServerCertificate", "true")
	case "verificar":
		q.Set("encrypt", "true")
		q.Set("TrustServerCertificate", "false")
		q.Set("hostNameInCertificate", c.Host)
		if c.SSLCA != "" {
			q.Set("certificate", c.SSLCA)
		}
	}
	u.RawQuery = q.Encode()
	return u.String(), nil
}

// dsnSQLite abre o arquivo: só leitura (mode=ro e query_only) sem escrita
// ligada na conexão.
func dsnSQLite(c Config) string {
	caminho := (&url.URL{Path: c.Arquivo}).EscapedPath()
	dsn := "file:" + caminho + "?_pragma=busy_timeout(5000)"
	if !c.Escrita {
		dsn += "&mode=ro&_pragma=query_only(1)"
	}
	return dsn
}

// abrir cria o *sql.DB da conexão para um banco.
func abrir(c Config, banco string) (*sql.DB, error) {
	switch c.Tipo {
	case Postgres:
		config, err := configPostgres(c, banco)
		if err != nil {
			return nil, err
		}
		return stdlib.OpenDB(*config), nil
	case MySQL:
		cfg, err := configMySQL(c, banco)
		if err != nil {
			return nil, err
		}
		conector, err := mysql.NewConnector(cfg)
		if err != nil {
			return nil, errors.New("configuração do MySQL recusada: " + redigir(err.Error(), c.Senha))
		}
		return sql.OpenDB(conector), nil
	case SQLServer:
		dsn, err := dsnSQLServer(c, banco)
		if err != nil {
			return nil, err
		}
		conector, err := mssql.NewConnector(dsn)
		if err != nil {
			return nil, errors.New("configuração do SQL Server recusada")
		}
		return sql.OpenDB(conector), nil
	case SQLite:
		return sql.Open("sqlite", dsnSQLite(c))
	}
	return nil, fmt.Errorf("tipo de banco desconhecido: %q", c.Tipo)
}

// Citar põe o identificador entre as aspas do dialeto, escapando as de dentro.
func Citar(dialeto, nome string) string {
	switch dialeto {
	case MySQL:
		return "`" + strings.ReplaceAll(nome, "`", "``") + "`"
	case SQLServer:
		return "[" + strings.ReplaceAll(nome, "]", "]]") + "]"
	}
	return `"` + strings.ReplaceAll(nome, `"`, `""`) + `"`
}

// SQLPrevia é o SELECT das primeiras linhas de uma tabela ou view.
func SQLPrevia(dialeto, banco, esquema, objeto string, limite int) string {
	n := strconv.Itoa(limite)
	switch dialeto {
	case Postgres:
		return "SELECT * FROM " + Citar(dialeto, esquema) + "." + Citar(dialeto, objeto) + " LIMIT " + n
	case MySQL:
		return "SELECT * FROM " + Citar(dialeto, banco) + "." + Citar(dialeto, objeto) + " LIMIT " + n
	case SQLServer:
		return "SELECT TOP " + n + " * FROM " + Citar(dialeto, banco) + "." + Citar(dialeto, esquema) + "." + Citar(dialeto, objeto)
	}
	return "SELECT * FROM " + Citar(dialeto, objeto) + " LIMIT " + n
}

// SQLGerado é o "Gerar SELECT" da árvore: o nome qualificado só quando precisa.
func SQLGerado(dialeto, esquema, objeto string, limite int) string {
	nome := Citar(dialeto, objeto)
	if simples(objeto) {
		nome = objeto
	}
	if esquema != "" && dialeto != MySQL && dialeto != SQLite {
		e := Citar(dialeto, esquema)
		if simples(esquema) {
			e = esquema
		}
		nome = e + "." + nome
	}
	n := strconv.Itoa(limite)
	if dialeto == SQLServer {
		return "SELECT TOP " + n + " * FROM " + nome + ";"
	}
	return "SELECT * FROM " + nome + " LIMIT " + n + ";"
}

func simples(nome string) bool {
	if nome == "" || (nome[0] >= '0' && nome[0] <= '9') {
		return false
	}
	for _, c := range nome {
		if !(c == '_' || (c >= 'a' && c <= 'z') || (c >= '0' && c <= '9')) {
			return false
		}
	}
	return true
}

// Versão e TLS da conexão (para o "Testar").

func versaoDoServidor(ctx context.Context, conn *sql.Conn, tipo string) string {
	var consulta string
	switch tipo {
	case Postgres:
		consulta = "SELECT version()"
	case MySQL:
		consulta = "SELECT VERSION()"
	case SQLServer:
		consulta = "SELECT CAST(SERVERPROPERTY('ProductVersion') AS nvarchar(128))"
	case SQLite:
		consulta = "SELECT sqlite_version()"
	}
	var v string
	if err := conn.QueryRowContext(ctx, consulta).Scan(&v); err != nil {
		return ""
	}
	switch tipo {
	case Postgres:
		// "PostgreSQL 16.4 on x86_64..." → "PostgreSQL 16.4".
		if partes := strings.Fields(v); len(partes) >= 2 {
			return partes[0] + " " + partes[1]
		}
	case MySQL:
		if strings.Contains(strings.ToLower(v), "mariadb") {
			return "MariaDB " + strings.SplitN(v, "-", 2)[0]
		}
		return "MySQL " + strings.SplitN(v, "-", 2)[0]
	case SQLServer:
		return "SQL Server " + v
	case SQLite:
		return "SQLite " + v
	}
	return v
}

func usouTLS(ctx context.Context, conn *sql.Conn, tipo string) bool {
	switch tipo {
	case Postgres:
		var cifrada bool
		conn.Raw(func(dc any) error {
			if c, ok := dc.(*stdlib.Conn); ok {
				_, cifrada = c.Conn().PgConn().Conn().(*tls.Conn)
			}
			return nil
		})
		return cifrada
	case MySQL:
		var nome, cifra string
		if conn.QueryRowContext(ctx, "SHOW SESSION STATUS LIKE 'Ssl_cipher'").Scan(&nome, &cifra) == nil {
			return cifra != ""
		}
	case SQLServer:
		var cifrada string
		if conn.QueryRowContext(ctx, "SELECT CAST(encrypt_option AS nvarchar(10)) FROM sys.dm_exec_connections WHERE session_id = @@SPID").Scan(&cifrada) == nil {
			return strings.EqualFold(cifrada, "TRUE")
		}
	}
	return false
}

// Catálogo (a árvore).

// Objeto é uma tabela ou view.
type Objeto struct {
	Nome string `json:"nome"`
	View bool   `json:"view,omitempty"`
}

// Coluna de uma tabela, para a árvore.
type ColunaTabela struct {
	Nome string `json:"nome"`
	Tipo string `json:"tipo"`
	PK   bool   `json:"pk,omitempty"`
	FK   bool   `json:"fk,omitempty"`
}

// MaxObjetos listados num esquema (o resto se acha pelo filtro... no banco).
const MaxObjetos = 20000

func consultarNomes(ctx context.Context, db *sql.DB, consulta string, args ...any) ([]string, error) {
	linhas, err := db.QueryContext(ctx, consulta, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	nomes := []string{}
	for linhas.Next() {
		var n string
		if err := linhas.Scan(&n); err != nil {
			return nil, err
		}
		nomes = append(nomes, n)
	}
	return nomes, linhas.Err()
}

func listarBancos(ctx context.Context, db *sql.DB, tipo string) ([]string, error) {
	switch tipo {
	case Postgres:
		return consultarNomes(ctx, db, `SELECT datname FROM pg_database WHERE datallowconn AND NOT datistemplate ORDER BY datname`)
	case MySQL:
		return consultarNomes(ctx, db, `SELECT SCHEMA_NAME FROM information_schema.SCHEMATA
			WHERE SCHEMA_NAME NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys') ORDER BY SCHEMA_NAME`)
	case SQLServer:
		return consultarNomes(ctx, db, `SELECT name FROM sys.databases WHERE state = 0 AND HAS_DBACCESS(name) = 1 ORDER BY name`)
	}
	return []string{}, nil
}

func listarEsquemas(ctx context.Context, db *sql.DB, tipo, banco string) ([]string, error) {
	switch tipo {
	case Postgres:
		return consultarNomes(ctx, db, `SELECT nspname FROM pg_namespace WHERE nspname NOT IN ('pg_catalog', 'information_schema')
			AND nspname NOT LIKE 'pg\_toast%' AND nspname NOT LIKE 'pg\_temp\_%' ORDER BY nspname`)
	case SQLServer:
		return consultarNomes(ctx, db, `SELECT name FROM `+Citar(SQLServer, banco)+`.sys.schemas
			WHERE name NOT IN ('sys', 'INFORMATION_SCHEMA', 'guest') AND name NOT LIKE 'db[_]%' ORDER BY name`)
	}
	return []string{}, nil
}

func listarObjetos(ctx context.Context, db *sql.DB, tipo, banco, esquema string) ([]Objeto, int, error) {
	var consulta, contagem string
	var args []any
	switch tipo {
	case Postgres:
		consulta = `SELECT c.relname, c.relkind IN ('v', 'm') FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
			WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f') AND NOT c.relispartition ORDER BY c.relname LIMIT ` + strconv.Itoa(MaxObjetos+1)
		contagem = `SELECT COUNT(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
			WHERE n.nspname = $1 AND c.relkind IN ('r', 'p', 'v', 'm', 'f') AND NOT c.relispartition`
		args = []any{esquema}
	case MySQL:
		consulta = `SELECT TABLE_NAME, TABLE_TYPE <> 'BASE TABLE' FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? ORDER BY TABLE_NAME LIMIT ` + strconv.Itoa(MaxObjetos+1)
		contagem = `SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA = ?`
		args = []any{banco}
	case SQLServer:
		b := Citar(SQLServer, banco)
		consulta = `SELECT TOP ` + strconv.Itoa(MaxObjetos+1) + ` o.name, CASE WHEN o.type = 'V' THEN 1 ELSE 0 END FROM ` + b + `.sys.objects o
			JOIN ` + b + `.sys.schemas s ON s.schema_id = o.schema_id WHERE s.name = @p1 AND o.type IN ('U', 'V') ORDER BY o.name`
		contagem = `SELECT COUNT(*) FROM ` + b + `.sys.objects o JOIN ` + b + `.sys.schemas s ON s.schema_id = o.schema_id WHERE s.name = @p1 AND o.type IN ('U', 'V')`
		args = []any{esquema}
	case SQLite:
		consulta = `SELECT name, type = 'view' FROM pragma_table_list WHERE schema = 'main' AND name NOT LIKE 'sqlite\_%' ESCAPE '\' ORDER BY name LIMIT ` + strconv.Itoa(MaxObjetos+1)
		contagem = `SELECT COUNT(*) FROM pragma_table_list WHERE schema = 'main' AND name NOT LIKE 'sqlite\_%' ESCAPE '\'`
	}
	linhas, err := db.QueryContext(ctx, consulta, args...)
	if err != nil {
		return nil, 0, err
	}
	defer linhas.Close()
	objetos := []Objeto{}
	for linhas.Next() {
		var o Objeto
		if err := linhas.Scan(&o.Nome, &o.View); err != nil {
			return nil, 0, err
		}
		objetos = append(objetos, o)
	}
	if err := linhas.Err(); err != nil {
		return nil, 0, err
	}
	total := len(objetos)
	if total > MaxObjetos {
		objetos = objetos[:MaxObjetos]
		if err := db.QueryRowContext(ctx, contagem, args...).Scan(&total); err != nil {
			return nil, 0, err
		}
	}
	return objetos, total, nil
}

func listarColunas(ctx context.Context, db *sql.DB, tipo, banco, esquema, objeto string) ([]ColunaTabela, error) {
	var consulta string
	var args []any
	switch tipo {
	case Postgres:
		consulta = `SELECT a.attname, format_type(a.atttypid, a.atttypmod),
			EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conrelid = c.oid AND k.contype = 'p' AND a.attnum = ANY (k.conkey)),
			EXISTS (SELECT 1 FROM pg_constraint k WHERE k.conrelid = c.oid AND k.contype = 'f' AND a.attnum = ANY (k.conkey))
			FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid JOIN pg_namespace n ON n.oid = c.relnamespace
			WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped ORDER BY a.attnum`
		args = []any{esquema, objeto}
	case MySQL:
		consulta = `SELECT c.COLUMN_NAME, c.COLUMN_TYPE, c.COLUMN_KEY = 'PRI',
			EXISTS (SELECT 1 FROM information_schema.KEY_COLUMN_USAGE k WHERE k.TABLE_SCHEMA = c.TABLE_SCHEMA AND k.TABLE_NAME = c.TABLE_NAME
				AND k.COLUMN_NAME = c.COLUMN_NAME AND k.REFERENCED_TABLE_NAME IS NOT NULL)
			FROM information_schema.COLUMNS c WHERE c.TABLE_SCHEMA = ? AND c.TABLE_NAME = ? ORDER BY c.ORDINAL_POSITION`
		args = []any{banco, objeto}
	case SQLServer:
		b := Citar(SQLServer, banco)
		consulta = `SELECT c.name, t.name,
			CASE WHEN EXISTS (SELECT 1 FROM ` + b + `.sys.index_columns ic JOIN ` + b + `.sys.indexes i ON i.object_id = ic.object_id AND i.index_id = ic.index_id
				WHERE i.is_primary_key = 1 AND ic.object_id = c.object_id AND ic.column_id = c.column_id) THEN 1 ELSE 0 END,
			CASE WHEN EXISTS (SELECT 1 FROM ` + b + `.sys.foreign_key_columns f WHERE f.parent_object_id = c.object_id AND f.parent_column_id = c.column_id) THEN 1 ELSE 0 END
			FROM ` + b + `.sys.columns c JOIN ` + b + `.sys.types t ON t.user_type_id = c.user_type_id
			JOIN ` + b + `.sys.objects o ON o.object_id = c.object_id JOIN ` + b + `.sys.schemas s ON s.schema_id = o.schema_id
			WHERE s.name = @p1 AND o.name = @p2 ORDER BY c.column_id`
		args = []any{esquema, objeto}
	case SQLite:
		consulta = `SELECT i.name, i.type, i.pk > 0, EXISTS (SELECT 1 FROM pragma_foreign_key_list(?1) f WHERE f."from" = i.name)
			FROM pragma_table_info(?1) i ORDER BY i.cid`
		args = []any{objeto}
	}
	linhas, err := db.QueryContext(ctx, consulta, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	colunas := []ColunaTabela{}
	for linhas.Next() {
		var c ColunaTabela
		if err := linhas.Scan(&c.Nome, &c.Tipo, &c.PK, &c.FK); err != nil {
			return nil, err
		}
		colunas = append(colunas, c)
	}
	return colunas, linhas.Err()
}

// existeObjeto confere no catálogo que a tabela ou view existe (a prévia só
// roda sobre um nome que o próprio banco listou).
func existeObjeto(ctx context.Context, db *sql.DB, tipo, banco, esquema, objeto string) (bool, error) {
	var consulta string
	var args []any
	switch tipo {
	case Postgres:
		consulta = `SELECT COUNT(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
			WHERE n.nspname = $1 AND c.relname = $2 AND c.relkind IN ('r', 'p', 'v', 'm', 'f')`
		args = []any{esquema, objeto}
	case MySQL:
		consulta = `SELECT COUNT(*) FROM information_schema.TABLES WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?`
		args = []any{banco, objeto}
	case SQLServer:
		b := Citar(SQLServer, banco)
		consulta = `SELECT COUNT(*) FROM ` + b + `.sys.objects o JOIN ` + b + `.sys.schemas s ON s.schema_id = o.schema_id
			WHERE s.name = @p1 AND o.name = @p2 AND o.type IN ('U', 'V')`
		args = []any{esquema, objeto}
	case SQLite:
		consulta = `SELECT COUNT(*) FROM pragma_table_list WHERE schema = 'main' AND name = ?`
		args = []any{objeto}
	}
	var n int
	if err := db.QueryRowContext(ctx, consulta, args...).Scan(&n); err != nil {
		return false, err
	}
	return n > 0, nil
}
