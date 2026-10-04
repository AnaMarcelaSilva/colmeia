package bancos

import (
	"context"
	"crypto/x509"
	"errors"
	"net"
	"strconv"
	"strings"

	"github.com/go-sql-driver/mysql"
	"github.com/jackc/pgx/v5/pgconn"
	mssql "github.com/microsoft/go-mssqldb"
)

var (
	// ErrSomenteLeitura: a instrução altera dados e a conexão não permite.
	ErrSomenteLeitura = errors.New("Esta conexão é somente leitura. Para alterar dados, ligue “Permitir alterações” na conexão.")
	// ErrTempoEsgotado: passou do tempo-limite e foi cancelada.
	ErrTempoEsgotado = errors.New("passou do tempo-limite e foi cancelada")
	// ErrCancelada: você cancelou.
	ErrCancelada = errors.New("consulta cancelada")
	// ErrSemExecucao: a execução não existe mais (fechada depois de parada).
	ErrSemExecucao = errors.New("a consulta foi fechada; execute de novo para ver mais")
	// ErrFichaEmUso: já há uma execução com essa ficha.
	ErrFichaEmUso = errors.New("já há uma execução com essa ficha")
	// ErrFechada: a conexão foi editada, removida ou desconectada no meio.
	ErrFechada = errors.New("a conexão foi fechada")
)

// ErrPrecisaConfirmar: a instrução altera dados e precisa da sua confirmação.
type ErrPrecisaConfirmar struct{ Classificacao }

func (e ErrPrecisaConfirmar) Error() string {
	return "a instrução " + e.Verbo + " altera dados e precisa de confirmação"
}

// ErrBanco é o erro que o servidor de banco devolveu, já sem a senha.
type ErrBanco struct {
	Mensagem string
	// Linha do SQL onde o servidor apontou o erro (0: sem posição).
	Linha int
	// causa: o erro do driver, para Explicar reconhecer o tipo (nunca vai
	// para fora: a mensagem é a redigida).
	causa error
}

func (e ErrBanco) Error() string { return e.Mensagem }
func (e ErrBanco) Unwrap() error { return e.causa }

// redigir troca a senha (se aparecer) por ***.
func redigir(texto, senha string) string {
	if len(senha) >= 3 {
		texto = strings.ReplaceAll(texto, senha, "***")
	}
	return texto
}

// erroDoBanco transforma o erro do driver no erro da Colmeia, sem a senha e,
// quando o servidor dá a posição, com a linha.
func erroDoBanco(err error, senha, sql string) error {
	if err == nil {
		return nil
	}
	for _, conhecido := range []error{ErrSomenteLeitura, ErrTempoEsgotado, ErrCancelada, ErrSemExecucao, ErrFechada} {
		if errors.Is(err, conhecido) {
			return conhecido
		}
	}
	e := ErrBanco{Mensagem: redigir(err.Error(), senha), causa: err}
	var pg *pgconn.PgError
	if errors.As(err, &pg) {
		e.Mensagem = redigir(pg.Severity+": "+pg.Message, senha)
		if pg.Detail != "" {
			e.Mensagem += "\n" + redigir(pg.Detail, senha)
		}
		if pg.Hint != "" {
			e.Mensagem += "\nDica: " + redigir(pg.Hint, senha)
		}
		if pg.Position > 0 && int(pg.Position) <= len([]rune(sql)) {
			e.Linha = strings.Count(string([]rune(sql)[:pg.Position-1]), "\n") + 1
		}
	}
	var my *mysql.MySQLError
	if errors.As(err, &my) {
		e.Mensagem = redigir("Erro "+strconv.Itoa(int(my.Number))+": "+my.Message, senha)
		// "... near 'x' at line 3"
		if i := strings.LastIndex(my.Message, " at line "); i >= 0 {
			e.Linha, _ = strconv.Atoi(strings.TrimSpace(my.Message[i+len(" at line "):]))
		}
	}
	var ms mssql.Error
	if errors.As(err, &ms) {
		e.Mensagem = redigir("Erro "+strconv.Itoa(int(ms.Number))+": "+ms.Message, senha)
		if ms.LineNo > 0 {
			e.Linha = int(ms.LineNo)
		}
	}
	return e
}

// SenhaRecusada: o servidor recusou usuário ou senha. O 1044 do MySQL (senha
// aceita, mas sem acesso ao banco) não entra: trocar a senha não resolveria.
func SenhaRecusada(err error) bool {
	var pg *pgconn.PgError
	var my *mysql.MySQLError
	var ms mssql.Error
	switch {
	case errors.As(err, &pg):
		return pg.Code == "28P01" || (pg.Code == "28000" && !strings.Contains(pg.Message, "no encryption"))
	case errors.As(err, &my):
		return my.Number == 1045
	case errors.As(err, &ms):
		return ms.Number == 18456
	}
	return false
}

// semPermissao: a frase do 1044, com o banco que o servidor citou ("... to
// database 'loja'") ou, sem ele, o da conexão.
func semPermissao(mensagem, banco string) string {
	const marca = "to database '"
	if i := strings.LastIndex(mensagem, marca); i >= 0 {
		resto := mensagem[i+len(marca):]
		if j := strings.IndexByte(resto, '\''); j > 0 {
			banco = resto[:j]
		}
	}
	if banco == "" {
		return "Sem permissão no banco."
	}
	return "Sem permissão no banco " + banco + "."
}

// Explicar traduz um erro de conexão numa frase simples, para o "Testar" e
// para a árvore. O detalhe (já sem a senha) fica para quem quiser ver.
func Explicar(err error, c Config) (frase, detalhe string) {
	if err == nil {
		return "", ""
	}
	detalhe = redigir(err.Error(), c.Senha)
	endereco := net.JoinHostPort(c.Host, strconv.Itoa(c.Porta))
	var pg *pgconn.PgError
	var my *mysql.MySQLError
	var x509Desconhecida x509.UnknownAuthorityError
	var x509Host x509.HostnameError
	var x509Invalido x509.CertificateInvalidError
	var rede *net.OpError
	var dns *net.DNSError
	minusculo := strings.ToLower(detalhe)
	switch {
	case SenhaRecusada(err):
		return "Usuário ou senha recusados.", detalhe
	case errors.As(err, &pg) && pg.Code == "28000":
		return "O servidor exige TLS: escolha Exigir.", detalhe
	case errors.As(err, &pg) && pg.Code == "3D000":
		return "O banco indicado não existe no servidor.", detalhe
	case errors.As(err, &my) && my.Number == 1044:
		return semPermissao(my.Message, c.Banco), detalhe
	case errors.As(err, &my) && my.Number == 3159:
		return "O servidor exige TLS: escolha Exigir.", detalhe
	case errors.As(err, &my) && my.Number == 1049:
		return "O banco indicado não existe no servidor.", detalhe
	case errors.As(err, &x509Desconhecida), errors.As(err, &x509Host), errors.As(err, &x509Invalido),
		strings.Contains(minusculo, "x509:"), strings.Contains(minusculo, "certificate"):
		return "O certificado não confere com a CA.", detalhe
	case strings.Contains(minusculo, "server does not support ssl") || strings.Contains(minusculo, "server refused tls") ||
		strings.Contains(minusculo, "tls requested but server does not support"):
		return "O servidor não aceita TLS: escolha Desligado ou Preferir.", detalhe
	case strings.Contains(minusculo, "requires secure transport") || strings.Contains(minusculo, "no encryption"):
		return "O servidor exige TLS: escolha Exigir.", detalhe
	case errors.As(err, &dns), errors.As(err, &rede), strings.Contains(minusculo, "connection refused"),
		strings.Contains(minusculo, "no such host"), strings.Contains(minusculo, "i/o timeout"), errors.Is(err, context.DeadlineExceeded),
		strings.Contains(minusculo, "failed to connect"), strings.Contains(minusculo, "unable to open tcp connection"):
		return "Não achou o servidor " + endereco + ".", detalhe
	case strings.Contains(minusculo, "unable to open database file") || strings.Contains(minusculo, "no such file"):
		return "Não consegui abrir o arquivo do SQLite.", detalhe
	}
	return "Não conectou.", detalhe
}
