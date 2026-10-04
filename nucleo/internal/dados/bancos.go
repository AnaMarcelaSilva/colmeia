package dados

import (
	"context"
	"crypto/rand"
	"database/sql"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// Conexões de banco de dados por perfil (versão 5 do banco). A senha não
// fica aqui: a coluna senha diz só onde ela está (no chaveiro do sistema, só
// na memória do núcleo ou em lugar nenhum). O histórico de consultas fica
// fora da corrente de eventos e pode ser apagado, como as mensagens (decisão
// 0006); os eventos dizem só que houve consulta ou alteração, sem o SQL e
// sem o resultado.
const esquemaBancos = `
CREATE TABLE IF NOT EXISTS conexoes_banco (
	id INTEGER PRIMARY KEY,
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	pasta TEXT NOT NULL DEFAULT '',
	nome TEXT NOT NULL,
	tipo TEXT NOT NULL CHECK (tipo IN ('postgres', 'mysql', 'sqlserver', 'sqlite')),
	host TEXT NOT NULL DEFAULT '',
	porta INTEGER NOT NULL DEFAULT 0,
	usuario TEXT NOT NULL DEFAULT '',
	banco TEXT NOT NULL DEFAULT '',
	arquivo TEXT NOT NULL DEFAULT '',
	ssl TEXT NOT NULL DEFAULT 'preferir' CHECK (ssl IN ('desligado', 'preferir', 'exigir', 'verificar')),
	ssl_ca TEXT NOT NULL DEFAULT '',
	escrita INTEGER NOT NULL DEFAULT 0,
	agentes INTEGER NOT NULL DEFAULT 0,
	chave_segredo TEXT NOT NULL,
	senha TEXT NOT NULL DEFAULT 'nenhuma' CHECK (senha IN ('chaveiro', 'memoria', 'nenhuma')),
	criada_em TEXT NOT NULL,
	atualizada_em TEXT NOT NULL,
	UNIQUE (perfil_id, nome)
);
CREATE TABLE IF NOT EXISTS consultas_banco (
	id INTEGER PRIMARY KEY,
	conexao_id INTEGER NOT NULL REFERENCES conexoes_banco(id) ON DELETE CASCADE,
	sql TEXT NOT NULL,
	banco TEXT NOT NULL DEFAULT '',
	origem TEXT NOT NULL CHECK (origem IN ('voce', 'agente')),
	agente_id INTEGER,
	momento TEXT NOT NULL,
	duracao_ms INTEGER NOT NULL DEFAULT 0,
	linhas INTEGER NOT NULL DEFAULT 0,
	erro INTEGER NOT NULL DEFAULT 0,
	altera INTEGER NOT NULL DEFAULT 0,
	resultado TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS consultas_por_conexao ON consultas_banco (conexao_id, id);
`

const (
	// MaxConsultas guardadas por conexão; as mais antigas saem.
	MaxConsultas = 500
	// MaxTextoConsulta em caracteres.
	MaxTextoConsulta = 20000
)

var (
	TiposBanco = []string{"postgres", "mysql", "sqlserver", "sqlite"}
	ModosSSL   = []string{"desligado", "preferir", "exigir", "verificar"}
	OndeSenha  = []string{"chaveiro", "memoria", "nenhuma"}
)

// PortaPadrao de cada tipo de banco.
func PortaPadrao(tipo string) int {
	switch tipo {
	case "postgres":
		return 5432
	case "mysql":
		return 3306
	case "sqlserver":
		return 1433
	}
	return 0
}

// ConexaoBanco é uma conexão de banco do perfil, sem a senha.
type ConexaoBanco struct {
	ID       int64  `json:"id"`
	PerfilID int64  `json:"perfil_id"`
	Pasta    string `json:"pasta"`
	Nome     string `json:"nome"`
	Tipo     string `json:"tipo"`
	Host     string `json:"host"`
	Porta    int    `json:"porta"`
	Usuario  string `json:"usuario"`
	Banco    string `json:"banco"`
	Arquivo  string `json:"arquivo"`
	SSL      string `json:"ssl"`
	SSLCA    string `json:"ssl_ca"`
	// Escrita: alterações permitidas (cada uma pede confirmação).
	Escrita bool `json:"escrita"`
	// Agentes: os agentes do perfil podem pedir consultas (você aprova cada uma).
	Agentes bool `json:"agentes"`
	// ChaveSegredo é a conta no chaveiro ("conexao:<uuid>"): aleatória, para
	// uma instância de teste nunca tocar nas entradas de outra.
	ChaveSegredo string `json:"-"`
	// Senha diz onde a senha está: chaveiro, memoria ou nenhuma.
	Senha        string `json:"senha"`
	CriadaEm     string `json:"criada_em"`
	AtualizadaEm string `json:"atualizada_em"`
}

// CamposConexao é o que a tela manda ao criar ou editar uma conexão.
type CamposConexao struct {
	Pasta   string `json:"pasta"`
	Nome    string `json:"nome"`
	Tipo    string `json:"tipo"`
	Host    string `json:"host"`
	Porta   int    `json:"porta"`
	Usuario string `json:"usuario"`
	Banco   string `json:"banco"`
	Arquivo string `json:"arquivo"`
	SSL     string `json:"ssl"`
	SSLCA   string `json:"ssl_ca"`
	Escrita bool   `json:"escrita"`
	Agentes bool   `json:"agentes"`
}

func semControle(campo, valor string, maximo int) (string, error) {
	if utf8.RuneCountInString(valor) > maximo {
		return "", ErrInvalido{fmt.Sprintf("%s pode ter no máximo %d caracteres", campo, maximo)}
	}
	if !utf8.ValidString(valor) || strings.ContainsFunc(valor, func(r rune) bool { return r < ' ' || r == 0x7f }) {
		return "", ErrInvalido{campo + " não pode ter quebras de linha nem caracteres de controle"}
	}
	return valor, nil
}

// arquivoValido confere um caminho absoluto de um arquivo que existe.
func arquivoValido(campo, caminho string) (string, error) {
	if caminho == "" {
		return "", ErrInvalido{campo + " não pode ficar vazio"}
	}
	if _, err := semControle(campo, caminho, 4096); err != nil {
		return "", err
	}
	if !filepath.IsAbs(caminho) {
		return "", ErrInvalido{campo + " precisa ser um caminho completo (começando com /)"}
	}
	info, err := os.Stat(caminho)
	if err != nil || !info.Mode().IsRegular() {
		return "", ErrInvalido{campo + " não existe ou não é um arquivo: " + caminho}
	}
	return filepath.Clean(caminho), nil
}

// ValidarConexao confere e normaliza os campos (porta padrão, espaços).
func ValidarConexao(c CamposConexao) (CamposConexao, error) {
	var err error
	if c.Nome, err = nomeValido("O nome da conexão", c.Nome, 80); err != nil {
		return c, err
	}
	if err = umDe("tipo", c.Tipo, TiposBanco); err != nil {
		return c, err
	}
	c.Pasta = strings.TrimSpace(c.Pasta)
	if c.Pasta, err = semControle("A pasta", c.Pasta, 80); err != nil {
		return c, err
	}
	if c.SSL == "" {
		c.SSL = "preferir"
	}
	if err = umDe("ssl", c.SSL, ModosSSL); err != nil {
		return c, err
	}
	if c.Tipo == "sqlite" {
		if c.Arquivo, err = arquivoValido("O arquivo do SQLite", strings.TrimSpace(c.Arquivo)); err != nil {
			return c, err
		}
		c.Host, c.Porta, c.Usuario, c.Banco, c.SSL, c.SSLCA = "", 0, "", "", "desligado", ""
		return c, nil
	}
	c.Arquivo = ""
	c.Host = strings.TrimSpace(c.Host)
	if c.Host == "" {
		return c, ErrInvalido{"informe o host do servidor"}
	}
	if len(c.Host) > 255 || strings.ContainsFunc(c.Host, func(r rune) bool {
		return r <= ' ' || r == '/' || r == ',' || r == '?' || r == '#' || r == '@' || r == 0x7f
	}) {
		return c, ErrInvalido{"host inválido: use só o nome ou o endereço do servidor, sem espaço, barra, vírgula ou @"}
	}
	if c.Porta == 0 {
		c.Porta = PortaPadrao(c.Tipo)
	}
	if c.Porta < 1 || c.Porta > 65535 {
		return c, ErrInvalido{"a porta precisa estar entre 1 e 65535"}
	}
	if c.Usuario, err = semControle("O usuário", strings.TrimSpace(c.Usuario), 128); err != nil {
		return c, err
	}
	if c.Banco, err = semControle("O banco", strings.TrimSpace(c.Banco), 128); err != nil {
		return c, err
	}
	c.SSLCA = strings.TrimSpace(c.SSLCA)
	if c.SSL != "verificar" {
		c.SSLCA = ""
	} else if c.SSLCA != "" {
		if c.SSLCA, err = arquivoValido("O certificado da CA", c.SSLCA); err != nil {
			return c, err
		}
	}
	return c, nil
}

func novaChaveSegredo() string {
	var b [16]byte
	rand.Read(b[:])
	b[6] = b[6]&0x0f | 0x40
	b[8] = b[8]&0x3f | 0x80
	return fmt.Sprintf("conexao:%x-%x-%x-%x-%x", b[0:4], b[4:6], b[6:8], b[8:10], b[10:16])
}

const colunasConexao = `id, perfil_id, pasta, nome, tipo, host, porta, usuario, banco, arquivo, ssl, ssl_ca, escrita, agentes, chave_segredo, senha, criada_em, atualizada_em`

func escanearConexao(l escaneavel, c *ConexaoBanco) error {
	return l.Scan(&c.ID, &c.PerfilID, &c.Pasta, &c.Nome, &c.Tipo, &c.Host, &c.Porta, &c.Usuario, &c.Banco, &c.Arquivo, &c.SSL, &c.SSLCA,
		&c.Escrita, &c.Agentes, &c.ChaveSegredo, &c.Senha, &c.CriadaEm, &c.AtualizadaEm)
}

func eventoConexao(acao string, c ConexaoBanco) map[string]any {
	return map[string]any{"acao": acao, "conexao_id": c.ID, "nome": c.Nome, "tipo": c.Tipo}
}

// ListarConexoes traz as conexões do perfil, por pasta e nome.
func (b *Banco) ListarConexoes(ctx context.Context, perfil int64) ([]ConexaoBanco, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasConexao+` FROM conexoes_banco WHERE perfil_id = ? ORDER BY pasta COLLATE NOCASE, nome COLLATE NOCASE`, perfil)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []ConexaoBanco{}
	for linhas.Next() {
		var c ConexaoBanco
		if err := escanearConexao(linhas, &c); err != nil {
			return nil, err
		}
		lista = append(lista, c)
	}
	return lista, linhas.Err()
}

// ConexaoBanco lê uma conexão.
func (b *Banco) ConexaoBanco(ctx context.Context, id int64) (ConexaoBanco, error) {
	var c ConexaoBanco
	err := escanearConexao(b.db.QueryRowContext(ctx, `SELECT `+colunasConexao+` FROM conexoes_banco WHERE id = ?`, id), &c)
	if errors.Is(err, sql.ErrNoRows) {
		return c, ErrNaoEncontrado
	}
	return c, err
}

// CriarConexao grava a conexão (sem senha: onde diz onde ela vai ficar).
func (b *Banco) CriarConexao(ctx context.Context, perfil int64, campos CamposConexao, onde string) (ConexaoBanco, error) {
	campos, err := ValidarConexao(campos)
	if err != nil {
		return ConexaoBanco{}, err
	}
	if err := umDe("senha", onde, OndeSenha); err != nil {
		return ConexaoBanco{}, err
	}
	if _, err := b.Perfil(ctx, perfil); err != nil {
		return ConexaoBanco{}, err
	}
	momento := agora()
	c := ConexaoBanco{PerfilID: perfil, Pasta: campos.Pasta, Nome: campos.Nome, Tipo: campos.Tipo, Host: campos.Host, Porta: campos.Porta,
		Usuario: campos.Usuario, Banco: campos.Banco, Arquivo: campos.Arquivo, SSL: campos.SSL, SSLCA: campos.SSLCA, Escrita: campos.Escrita,
		Agentes: campos.Agentes, ChaveSegredo: novaChaveSegredo(), Senha: onde, CriadaEm: momento, AtualizadaEm: momento}
	err = b.emTransacao(ctx, func(tx *transacao) error {
		r, err := tx.ExecContext(ctx, `INSERT INTO conexoes_banco (perfil_id, pasta, nome, tipo, host, porta, usuario, banco, arquivo, ssl, ssl_ca,
			escrita, agentes, chave_segredo, senha, criada_em, atualizada_em) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			c.PerfilID, c.Pasta, c.Nome, c.Tipo, c.Host, c.Porta, c.Usuario, c.Banco, c.Arquivo, c.SSL, c.SSLCA, c.Escrita, c.Agentes,
			c.ChaveSegredo, c.Senha, c.CriadaEm, c.AtualizadaEm)
		if err != nil {
			return traduzir(err)
		}
		c.ID, _ = r.LastInsertId()
		return registrar(ctx, tx, "banco.conexao", Escopo{Perfil: perfil}, eventoConexao("criada", c))
	})
	return c, err
}

// AtualizarConexao troca os campos da conexão (a senha fica onde estava).
func (b *Banco) AtualizarConexao(ctx context.Context, id int64, campos CamposConexao) (ConexaoBanco, error) {
	campos, err := ValidarConexao(campos)
	if err != nil {
		return ConexaoBanco{}, err
	}
	var c ConexaoBanco
	err = b.emTransacao(ctx, func(tx *transacao) error {
		if err := escanearConexao(tx.QueryRowContext(ctx, `SELECT `+colunasConexao+` FROM conexoes_banco WHERE id = ?`, id), &c); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return ErrNaoEncontrado
			}
			return err
		}
		if c.Tipo != campos.Tipo {
			return ErrInvalido{"o tipo da conexão não muda; crie uma conexão nova"}
		}
		c.Pasta, c.Nome, c.Host, c.Porta, c.Usuario, c.Banco, c.Arquivo = campos.Pasta, campos.Nome, campos.Host, campos.Porta, campos.Usuario, campos.Banco, campos.Arquivo
		c.SSL, c.SSLCA, c.Escrita, c.Agentes, c.AtualizadaEm = campos.SSL, campos.SSLCA, campos.Escrita, campos.Agentes, agora()
		if _, err := tx.ExecContext(ctx, `UPDATE conexoes_banco SET pasta = ?, nome = ?, host = ?, porta = ?, usuario = ?, banco = ?, arquivo = ?,
			ssl = ?, ssl_ca = ?, escrita = ?, agentes = ?, atualizada_em = ? WHERE id = ?`,
			c.Pasta, c.Nome, c.Host, c.Porta, c.Usuario, c.Banco, c.Arquivo, c.SSL, c.SSLCA, c.Escrita, c.Agentes, c.AtualizadaEm, id); err != nil {
			return traduzir(err)
		}
		return registrar(ctx, tx, "banco.conexao", Escopo{Perfil: c.PerfilID}, eventoConexao("alterada", c))
	})
	return c, err
}

// DefinirOndeSenha grava onde a senha da conexão está agora (sem evento: a
// senha não é história).
func (b *Banco) DefinirOndeSenha(ctx context.Context, id int64, onde string) error {
	if err := umDe("senha", onde, OndeSenha); err != nil {
		return err
	}
	r, err := b.db.ExecContext(ctx, `UPDATE conexoes_banco SET senha = ? WHERE id = ?`, onde, id)
	if err != nil {
		return err
	}
	if n, _ := r.RowsAffected(); n == 0 {
		return ErrNaoEncontrado
	}
	return nil
}

// RemoverConexao apaga a conexão e o histórico de consultas dela (a linha do
// tempo continua). Devolve a conexão removida, para a senha sair do chaveiro.
func (b *Banco) RemoverConexao(ctx context.Context, id int64) (ConexaoBanco, error) {
	var c ConexaoBanco
	err := b.emTransacao(ctx, func(tx *transacao) error {
		if err := escanearConexao(tx.QueryRowContext(ctx, `SELECT `+colunasConexao+` FROM conexoes_banco WHERE id = ?`, id), &c); err != nil {
			if errors.Is(err, sql.ErrNoRows) {
				return ErrNaoEncontrado
			}
			return err
		}
		if _, err := tx.ExecContext(ctx, `DELETE FROM conexoes_banco WHERE id = ?`, id); err != nil {
			return err
		}
		return registrar(ctx, tx, "banco.conexao", Escopo{Perfil: c.PerfilID}, eventoConexao("removida", c))
	})
	return c, err
}

// ConsultaBanco é uma linha do histórico de consultas de uma conexão.
type ConsultaBanco struct {
	ID        int64  `json:"id"`
	ConexaoID int64  `json:"conexao_id"`
	SQL       string `json:"sql"`
	Banco     string `json:"banco"`
	Origem    string `json:"origem"`
	AgenteID  int64  `json:"agente_id,omitempty"`
	Momento   string `json:"momento"`
	DuracaoMs int64  `json:"duracao_ms"`
	Linhas    int64  `json:"linhas"`
	Erro      bool   `json:"erro"`
	Altera    bool   `json:"altera"`
	// Resultado de um pedido do agente (aprovada, recusada, expirou) ou
	// "cancelada" (pelo agente ou por você, no console).
	Resultado string `json:"resultado,omitempty"`
	// Hora local para a tela: "14:32" hoje, "02/10" antes.
	Hora string `json:"hora,omitempty"`
}

// senhaNoSQL: CREATE/ALTER USER ... PASSWORD '...', IDENTIFIED BY '...',
// WITH PASSWORD = '...' (SQL Server).
var senhaNoSQL = regexp.MustCompile(`(?i)\b(password|identified\s+(with\s+\S+\s+)?by)\s*=?\s*(N?'|")`)

// PareceSegredoSQL diz se o SQL parece levar uma senha.
func PareceSegredoSQL(sql string) bool {
	return segredos.Parece(sql) || senhaNoSQL.MatchString(sql)
}

// GuardarConsulta acrescenta ao histórico da conexão e diz se guardou. Não
// guarda o que parece ter um segredo (CREATE USER ... PASSWORD): a consulta é
// executada, só não fica no histórico. Passou de MaxConsultas, as mais
// antigas saem.
func (b *Banco) GuardarConsulta(ctx context.Context, c ConsultaBanco) (bool, error) {
	if strings.TrimSpace(c.SQL) == "" || PareceSegredoSQL(c.SQL) {
		return false, nil
	}
	if r := []rune(c.SQL); len(r) > MaxTextoConsulta {
		c.SQL = string(r[:MaxTextoConsulta])
	}
	if c.Momento == "" {
		c.Momento = agora()
	}
	err := b.emTransacao(ctx, func(tx *transacao) error {
		if _, err := tx.ExecContext(ctx, `INSERT INTO consultas_banco (conexao_id, sql, banco, origem, agente_id, momento, duracao_ms, linhas, erro, altera, resultado)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`, c.ConexaoID, c.SQL, c.Banco, c.Origem, nulo(c.AgenteID), c.Momento, c.DuracaoMs, c.Linhas, c.Erro,
			c.Altera, c.Resultado); err != nil {
			return err
		}
		_, err := tx.ExecContext(ctx, `DELETE FROM consultas_banco WHERE conexao_id = ? AND id NOT IN
			(SELECT id FROM consultas_banco WHERE conexao_id = ? ORDER BY id DESC LIMIT ?)`, c.ConexaoID, c.ConexaoID, MaxConsultas)
		return err
	})
	return err == nil, err
}

// ListarConsultas traz o histórico da conexão, da mais nova para a mais antiga.
func (b *Banco) ListarConsultas(ctx context.Context, conexao int64, limite int) ([]ConsultaBanco, error) {
	if limite <= 0 || limite > MaxConsultas {
		limite = MaxConsultas
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT id, conexao_id, sql, banco, origem, COALESCE(agente_id, 0), momento, duracao_ms, linhas, erro, altera, resultado
		FROM consultas_banco WHERE conexao_id = ? ORDER BY id DESC LIMIT ?`, conexao, limite)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []ConsultaBanco{}
	for linhas.Next() {
		var c ConsultaBanco
		if err := linhas.Scan(&c.ID, &c.ConexaoID, &c.SQL, &c.Banco, &c.Origem, &c.AgenteID, &c.Momento, &c.DuracaoMs, &c.Linhas, &c.Erro, &c.Altera, &c.Resultado); err != nil {
			return nil, err
		}
		lista = append(lista, c)
	}
	return lista, linhas.Err()
}

// LimparConsultas apaga o histórico da conexão.
func (b *Banco) LimparConsultas(ctx context.Context, conexao int64) error {
	_, err := b.db.ExecContext(ctx, `DELETE FROM consultas_banco WHERE conexao_id = ?`, conexao)
	return err
}
