package bancos

import (
	"errors"
	"strings"
	"unicode"
	"unicode/utf8"
)

// Leitor léxico de SQL, um por dialeto. Não entende a gramática: só separa
// texto, identificadores entre aspas, comentários e palavras, o bastante para
// dividir as instruções, achar a que está sob o cursor e decidir se uma
// instrução só lê ou se altera dados. Na dúvida, "altera": errar para esse
// lado só faz aparecer uma confirmação a mais.

// Dialetos (os mesmos nomes dos tipos de conexão).
const (
	Postgres  = "postgres"
	MySQL     = "mysql"
	SQLServer = "sqlserver"
	SQLite    = "sqlite"
)

// Tipos de pedaço.
const (
	PedacoEspaco = iota
	PedacoPalavra
	PedacoTexto  // '...', $tag$...$tag$
	PedacoCitado // "x", `x`, [x]
	PedacoNumero
	PedacoComentario
	PedacoSimbolo
)

// Pedaco é um trecho do SQL, com as posições em bytes.
type Pedaco struct {
	Tipo     int
	Ini, Fim int
}

// Pedacos divide o SQL inteiro. Um texto ou comentário sem fim vai até o fim.
func Pedacos(dialeto, sql string) []Pedaco {
	var lista []Pedaco
	i := 0
	// Dentro de /*! ... */ do MySQL: o conteúdo é código que o servidor roda.
	executavel := false
	for i < len(sql) {
		c := sql[i]
		ini := i
		tipo := PedacoSimbolo
		switch {
		case c == ' ' || c == '\t' || c == '\n' || c == '\r' || c == '\f' || c == '\v':
			for i < len(sql) && strings.IndexByte(" \t\n\r\f\v", sql[i]) >= 0 {
				i++
			}
			tipo = PedacoEspaco
		case executavel && c == '*' && i+1 < len(sql) && sql[i+1] == '/':
			i += 2
			executavel = false
			tipo = PedacoEspaco
		case c == '-' && i+1 < len(sql) && sql[i+1] == '-' && (dialeto != MySQL || i+2 >= len(sql) || sql[i+2] <= ' '):
			i = fimDaLinha(sql, i)
			tipo = PedacoComentario
		case c == '#' && dialeto == MySQL:
			i = fimDaLinha(sql, i)
			tipo = PedacoComentario
		case c == '/' && i+1 < len(sql) && sql[i+1] == '*':
			if dialeto == MySQL && i+2 < len(sql) && sql[i+2] == '!' {
				// /*!50000 ... */: o número de versão some, o resto é código.
				i += 3
				for i < len(sql) && sql[i] >= '0' && sql[i] <= '9' {
					i++
				}
				executavel = true
				tipo = PedacoEspaco
				break
			}
			i = fimDoComentario(dialeto, sql, i)
			tipo = PedacoComentario
		case c == '\'':
			i = fimDoTexto(sql, i, '\'', dialeto == MySQL || precedidoPorE(sql, ini))
			tipo = PedacoTexto
		case c == '"':
			i = fimDoTexto(sql, i, '"', dialeto == MySQL)
			tipo = PedacoCitado
			if dialeto == MySQL {
				tipo = PedacoTexto
			}
		case c == '`' && (dialeto == MySQL || dialeto == SQLite):
			i = fimDoTexto(sql, i, '`', false)
			tipo = PedacoCitado
		case c == '[' && (dialeto == SQLServer || dialeto == SQLite):
			i = fimDoTexto(sql, i, ']', false)
			tipo = PedacoCitado
		case c == '$' && dialeto == Postgres && !colado(sql, i):
			if fim, ok := fimDoCifrao(sql, i); ok {
				i = fim
				tipo = PedacoTexto
			} else {
				i++
			}
		case c >= '0' && c <= '9':
			for i < len(sql) && (isAlnum(sql[i]) || sql[i] == '.') {
				i++
			}
			tipo = PedacoNumero
		case isInicioPalavra(sql, i):
			for i < len(sql) {
				r, n := utf8.DecodeRuneInString(sql[i:])
				if !(r == '_' || r == '$' || r == '@' || r == '#' || unicode.IsLetter(r) || unicode.IsDigit(r)) {
					break
				}
				i += n
			}
			tipo = PedacoPalavra
		default:
			_, n := utf8.DecodeRuneInString(sql[i:])
			i += n
		}
		// Espaços e comentários seguidos viram um só pedaço de cada tipo.
		if k := len(lista) - 1; k >= 0 && tipo == PedacoEspaco && lista[k].Tipo == PedacoEspaco {
			lista[k].Fim = i
			continue
		}
		lista = append(lista, Pedaco{Tipo: tipo, Ini: ini, Fim: i})
	}
	return lista
}

func isAlnum(c byte) bool {
	return c == '_' || (c >= '0' && c <= '9') || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')
}

func isInicioPalavra(sql string, i int) bool {
	r, _ := utf8.DecodeRuneInString(sql[i:])
	return r == '_' || r == '@' || r == '#' || unicode.IsLetter(r)
}

// colado: o $ faz parte de um nome (a$b) ou de um parâmetro ($1).
func colado(sql string, i int) bool {
	if i > 0 && isAlnum(sql[i-1]) {
		return true
	}
	return i+1 < len(sql) && sql[i+1] >= '0' && sql[i+1] <= '9'
}

// precedidoPorE: E'...' no Postgres aceita barra invertida como escape.
func precedidoPorE(sql string, i int) bool {
	return i > 0 && (sql[i-1] == 'E' || sql[i-1] == 'e') && (i == 1 || !isAlnum(sql[i-2]))
}

func fimDaLinha(sql string, i int) int {
	if n := strings.IndexByte(sql[i:], '\n'); n >= 0 {
		return i + n
	}
	return len(sql)
}

// fimDoComentario: o Postgres e o SQL Server aceitam /* */ aninhado.
func fimDoComentario(dialeto, sql string, i int) int {
	nivel := 0
	for i < len(sql) {
		switch {
		case strings.HasPrefix(sql[i:], "/*"):
			if nivel == 0 || dialeto == Postgres || dialeto == SQLServer {
				nivel++
			}
			i += 2
		case strings.HasPrefix(sql[i:], "*/"):
			nivel--
			i += 2
			if nivel == 0 {
				return i
			}
		default:
			i++
		}
	}
	return len(sql)
}

// fimDoTexto acha o fechamento; o fechamento dobrado (”) é escape.
func fimDoTexto(sql string, i int, fecha byte, barra bool) int {
	i++
	for i < len(sql) {
		switch {
		case barra && sql[i] == '\\':
			i += 2
		case sql[i] == fecha:
			if i+1 < len(sql) && sql[i+1] == fecha {
				i += 2
				continue
			}
			return i + 1
		default:
			i++
		}
	}
	return len(sql)
}

// fimDoCifrao: $$...$$ ou $tag$...$tag$ do Postgres.
func fimDoCifrao(sql string, i int) (int, bool) {
	j := i + 1
	for j < len(sql) && isAlnum(sql[j]) {
		j++
	}
	if j >= len(sql) || sql[j] != '$' {
		return 0, false
	}
	marca := sql[i : j+1]
	if n := strings.Index(sql[j+1:], marca); n >= 0 {
		return j + 1 + n + len(marca), true
	}
	return len(sql), true
}

// Trecho é uma instrução dentro do texto, sem os espaços em volta e sem o ;.
type Trecho struct {
	Ini int `json:"ini"`
	Fim int `json:"fim"`
}

// Dividir separa as instruções pelos ; fora de texto e comentário. Trechos
// só com espaço e comentário ficam de fora.
func Dividir(dialeto, sql string) []Trecho {
	var trechos []Trecho
	ini, fim := -1, -1
	fechar := func() {
		if ini >= 0 {
			trechos = append(trechos, Trecho{ini, fim})
		}
		ini, fim = -1, -1
	}
	for _, p := range Pedacos(dialeto, sql) {
		switch {
		case p.Tipo == PedacoSimbolo && sql[p.Ini:p.Fim] == ";":
			fechar()
		case p.Tipo == PedacoEspaco || p.Tipo == PedacoComentario:
		default:
			if ini < 0 {
				ini = p.Ini
			}
			fim = p.Fim
		}
	}
	fechar()
	return trechos
}

// SobCursor escolhe a instrução do cursor (posição em bytes). Fora de todas,
// vale a última que termina antes dele; sem ela, a primeira depois.
func SobCursor(dialeto, sql string, pos int) (Trecho, bool) {
	trechos := Dividir(dialeto, sql)
	if len(trechos) == 0 {
		return Trecho{}, false
	}
	escolhido := -1
	for i, t := range trechos {
		if pos >= t.Ini && pos <= t.Fim {
			return t, true
		}
		if t.Fim <= pos {
			escolhido = i
		}
	}
	if escolhido < 0 {
		escolhido = 0
	}
	return trechos[escolhido], true
}

// Classe de uma instrução.
const (
	Leitura = "leitura"
	Altera  = "altera"
)

// Classificacao é o que o núcleo decide de uma instrução.
type Classificacao struct {
	Classe string
	// Verbo é a palavra que diz o que ela faz (SELECT, UPDATE, CREATE...).
	Verbo string
	// SemWhere: UPDATE ou DELETE sem WHERE (altera a tabela inteira).
	SemWhere bool
}

var (
	ErrVazia  = errors.New("não há instrução para executar")
	ErrVarias = errors.New("há mais de uma instrução; a Colmeia executa uma por vez")
)

// Primeiras palavras de quem só lê.
var primeirasDeLeitura = map[string]bool{
	"SELECT": true, "WITH": true, "SHOW": true, "DESCRIBE": true, "DESC": true, "EXPLAIN": true, "VALUES": true, "TABLE": true,
}

// Palavras que, em qualquer ponto fora de texto, fazem a instrução contar
// como alteração.
var palavrasQueAlteram = map[string]bool{
	"INSERT": true, "UPDATE": true, "DELETE": true, "MERGE": true, "REPLACE": true, "UPSERT": true, "CREATE": true, "ALTER": true,
	"DROP": true, "TRUNCATE": true, "GRANT": true, "REVOKE": true, "CALL": true, "EXEC": true, "EXECUTE": true, "COPY": true,
	"LOCK": true, "LOAD": true, "HANDLER": true, "RENAME": true, "INTO": true, "SET": true, "ATTACH": true, "DETACH": true,
	"VACUUM": true, "REINDEX": true, "OPENROWSET": true, "OPENQUERY": true, "OPENDATASOURCE": true, "DO": true,
}

// Funções que, chamadas como função (palavra seguida de "("), só leem.
var funcoesDeLeitura = map[string]bool{"REPLACE": true, "INSERT": true}

// Classificar decide se a instrução (uma só) lê ou altera dados.
func Classificar(dialeto, sql string) (Classificacao, error) {
	trechos := Dividir(dialeto, sql)
	if len(trechos) == 0 {
		return Classificacao{}, ErrVazia
	}
	if len(trechos) > 1 {
		return Classificacao{}, ErrVarias
	}
	palavras, seguidas := palavrasDe(dialeto, sql)
	if len(palavras) == 0 {
		// Começa com símbolo que não é parêntese: na dúvida, altera.
		return Classificacao{Classe: Altera, Verbo: "?"}, nil
	}
	primeira := palavras[0]
	c := Classificacao{Classe: Leitura, Verbo: primeira}
	leitura := primeirasDeLeitura[primeira] || (primeira == "PRAGMA" && dialeto == SQLite && pragmaSoLe(dialeto, sql))
	alteraEm := ""
	for i, p := range palavras {
		if !palavrasQueAlteram[p] || (funcoesDeLeitura[p] && seguidas[i] == "(") {
			// FOR UPDATE / FOR SHARE / FOR KEY SHARE / LOCK IN SHARE MODE.
			if p == "SHARE" && i > 0 && (palavras[i-1] == "FOR" || palavras[i-1] == "KEY" || palavras[i-1] == "IN") {
				alteraEm = p
			}
			if p == "ANALYZE" && primeira == "EXPLAIN" {
				alteraEm = p
			}
			if alteraEm != "" {
				break
			}
			continue
		}
		alteraEm = p
		break
	}
	if !leitura || alteraEm != "" {
		c.Classe = Altera
		// WITH ... DELETE: o verbo é o que altera.
		if primeirasDeLeitura[primeira] && alteraEm != "" && alteraEm != "INTO" && alteraEm != "SHARE" && alteraEm != "SET" && alteraEm != "ANALYZE" {
			c.Verbo = alteraEm
		}
	}
	if primeira == "UPDATE" || primeira == "DELETE" {
		c.SemWhere = true
		for _, p := range palavras {
			if p == "WHERE" {
				c.SemWhere = false
				break
			}
		}
	}
	return c, nil
}

// palavrasDe devolve as palavras em maiúsculas (fora de texto, citação e
// comentário) e, para cada uma, o símbolo que vem logo depois ("" se não
// vier símbolo). Parênteses e ; no começo são pulados.
func palavrasDe(dialeto, sql string) ([]string, []string) {
	pedacos := Pedacos(dialeto, sql)
	var palavras, seguidas []string
	comecou := false
	for i, p := range pedacos {
		switch p.Tipo {
		case PedacoEspaco, PedacoComentario:
			continue
		case PedacoPalavra:
			comecou = true
			palavras = append(palavras, strings.ToUpper(sql[p.Ini:p.Fim]))
			proximo := ""
			for _, q := range pedacos[i+1:] {
				if q.Tipo == PedacoEspaco || q.Tipo == PedacoComentario {
					continue
				}
				if q.Tipo == PedacoSimbolo {
					proximo = sql[q.Ini:q.Fim]
				}
				break
			}
			seguidas = append(seguidas, proximo)
		default:
			if !comecou && !(p.Tipo == PedacoSimbolo && (sql[p.Ini:p.Fim] == "(" || sql[p.Ini:p.Fim] == ";")) {
				// Começa com algo que não é palavra nem parêntese.
				return nil, nil
			}
			comecou = true
		}
	}
	return palavras, seguidas
}

// pragmaSoLe: PRAGMA x ou PRAGMA x(tabela) com nome de tabela só leem; com
// "=" ou com um valor entre parênteses que não é nome, mudam a configuração.
func pragmaSoLe(dialeto, sql string) bool {
	pedacos := Pedacos(dialeto, sql)
	for _, p := range pedacos {
		if p.Tipo == PedacoSimbolo && sql[p.Ini:p.Fim] == "=" {
			return false
		}
		if p.Tipo == PedacoNumero {
			return false
		}
	}
	nome := ""
	for _, p := range pedacos {
		if p.Tipo == PedacoPalavra && !strings.EqualFold(sql[p.Ini:p.Fim], "PRAGMA") {
			nome = strings.ToLower(sql[p.Ini:p.Fim])
			break
		}
	}
	return pragmasDeLeitura[nome]
}

// Pragmas do SQLite que só informam.
var pragmasDeLeitura = map[string]bool{
	"table_info": true, "table_xinfo": true, "table_list": true, "index_list": true, "index_info": true, "index_xinfo": true,
	"foreign_key_list": true, "database_list": true, "collation_list": true, "function_list": true, "module_list": true,
	"pragma_list": true, "compile_options": true, "page_count": true, "page_size": true, "user_version": true,
	"schema_version": true, "application_id": true, "encoding": true, "journal_mode": true, "freelist_count": true,
	"integrity_check": true, "quick_check": true, "foreign_key_check": true,
}

// Funções que o agente não pode chamar nem numa consulta de leitura: leem
// arquivos do servidor, abrem conexões a outros bancos ou derrubam sessões.
var funcoesNegadasAoAgente = []string{
	"pg_terminate_backend", "pg_cancel_backend", "pg_reload_conf", "pg_rotate_logfile", "set_config", "dblink", "lo_",
	"pg_read_file", "pg_read_binary_file", "pg_ls_", "pg_stat_file", "pg_logical_", "pg_replication_", "pg_create_",
	"pg_drop_", "pg_promote", "pg_switch_wal", "pg_advisory", "load_file", "load_extension", "readfile", "writefile",
	"sys_exec", "sys_eval", "xp_", "sp_",
}

// FuncaoNegadaAoAgente devolve a primeira função proibida ao agente usada no SQL.
func FuncaoNegadaAoAgente(dialeto, sql string) string {
	for _, p := range Pedacos(dialeto, sql) {
		if p.Tipo != PedacoPalavra {
			continue
		}
		palavra := strings.ToLower(sql[p.Ini:p.Fim])
		for _, f := range funcoesNegadasAoAgente {
			if palavra == f || (strings.HasSuffix(f, "_") && strings.HasPrefix(palavra, f)) || (f == "dblink" && strings.HasPrefix(palavra, f)) {
				return palavra
			}
		}
	}
	return ""
}
