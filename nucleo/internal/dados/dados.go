// Package dados guarda perfis, contas de IA, workspaces, projetos e tarefas
// num SQLite local. Toda mudança também entra no histórico de eventos, onde
// cada evento carrega o hash do anterior: uma alteração posterior no histórico
// fica visível.
package dados

import (
	"context"
	"crypto/sha256"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"time"
	"unicode/utf8"

	_ "modernc.org/sqlite"
)

var (
	ErrNaoEncontrado = errors.New("não encontrado")
	ErrJaExiste      = errors.New("já existe")
)

// ErrInvalido explica ao usuário o que está errado no pedido.
type ErrInvalido struct{ Motivo string }

func (e ErrInvalido) Error() string { return e.Motivo }

const esquema = `
CREATE TABLE IF NOT EXISTS perfis (
	id INTEGER PRIMARY KEY,
	nome TEXT NOT NULL UNIQUE,
	tema TEXT NOT NULL DEFAULT 'escuro' CHECK (tema IN ('escuro', 'claro', 'leitura')),
	criado_em TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS contas_ia (
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	ferramenta TEXT NOT NULL,
	modo TEXT NOT NULL CHECK (modo IN ('sistema', 'separada')),
	PRIMARY KEY (perfil_id, ferramenta)
);
CREATE TABLE IF NOT EXISTS workspaces (
	id INTEGER PRIMARY KEY,
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	nome TEXT NOT NULL,
	UNIQUE (perfil_id, nome)
);
CREATE TABLE IF NOT EXISTS projetos (
	id INTEGER PRIMARY KEY,
	workspace_id INTEGER NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
	nome TEXT NOT NULL,
	caminho TEXT NOT NULL,
	branch_padrao TEXT NOT NULL,
	UNIQUE (workspace_id, nome)
);
CREATE TABLE IF NOT EXISTS tarefas (
	id INTEGER PRIMARY KEY,
	projeto_id INTEGER NOT NULL REFERENCES projetos(id) ON DELETE CASCADE,
	titulo TEXT NOT NULL,
	coluna TEXT NOT NULL CHECK (coluna IN ('backlog', 'trabalhando', 'aguardando', 'revisao', 'concluido')),
	branch TEXT NOT NULL,
	ordem REAL NOT NULL,
	criado_em TEXT NOT NULL,
	atualizado_em TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS tarefas_por_projeto ON tarefas (projeto_id, coluna, ordem);
CREATE TABLE IF NOT EXISTS agentes (
	id INTEGER PRIMARY KEY,
	tarefa_id INTEGER NOT NULL REFERENCES tarefas(id) ON DELETE CASCADE,
	ferramenta TEXT NOT NULL,
	papel TEXT NOT NULL,
	sessao TEXT NOT NULL DEFAULT '',
	criado_em TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS eventos (
	id INTEGER PRIMARY KEY,
	momento TEXT NOT NULL,
	tipo TEXT NOT NULL,
	dados TEXT NOT NULL,
	hash_anterior TEXT NOT NULL,
	hash TEXT NOT NULL
);
`

type Banco struct{ db *sql.DB }

// Diretorio é onde ficam os dados: COLMEIA_DADOS, ou $XDG_DATA_HOME/colmeia,
// ou ~/.local/share/colmeia.
func Diretorio() (string, error) {
	if d := os.Getenv("COLMEIA_DADOS"); d != "" {
		return d, nil
	}
	if d := os.Getenv("XDG_DATA_HOME"); d != "" {
		return filepath.Join(d, "colmeia"), nil
	}
	casa, err := os.UserHomeDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(casa, ".local", "share", "colmeia"), nil
}

// Abrir cria o diretório (0700) e o banco, e aplica o esquema.
func Abrir(dir string) (*Banco, error) {
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return nil, err
	}
	if err := os.Chmod(dir, 0o700); err != nil {
		return nil, err
	}
	caminho := filepath.Join(dir, "colmeia.db")
	db, err := sql.Open("sqlite", "file:"+caminho+"?_pragma=foreign_keys(1)&_pragma=journal_mode(WAL)&_pragma=busy_timeout(5000)")
	if err != nil {
		return nil, err
	}
	// Uma conexão só: o SQLite serializa as escritas de qualquer forma, e assim
	// o encadeamento dos eventos nunca disputa com outra transação.
	db.SetMaxOpenConns(1)
	if _, err := db.Exec(esquema); err != nil {
		db.Close()
		return nil, fmt.Errorf("aplicando o esquema: %w", err)
	}
	if err := migrar(db); err != nil {
		db.Close()
		return nil, fmt.Errorf("atualizando o banco: %w", err)
	}
	for _, sufixo := range []string{"", "-wal", "-shm"} {
		os.Chmod(caminho+sufixo, 0o600)
	}
	return &Banco{db: db}, nil
}

func (b *Banco) Fechar() error { return b.db.Close() }

// migrar acrescenta as colunas que surgiram depois da primeira versão, sem
// perder o que já está gravado.
func migrar(db *sql.DB) error {
	colunas := []struct{ tabela, coluna, definicao string }{
		// "git" (repositório) ou "pasta" (pasta de trabalho sem git).
		{"projetos", "tipo", "TEXT NOT NULL DEFAULT 'git'"},
		// Onde os agentes da tarefa trabalham: "pasta" (a do projeto) ou "copia" (worktree).
		{"tarefas", "local", "TEXT NOT NULL DEFAULT 'pasta'"},
		{"tarefas", "copia", "TEXT NOT NULL DEFAULT ''"},
	}
	for _, c := range colunas {
		existe := false
		linhas, err := db.Query(`SELECT name FROM pragma_table_info(?)`, c.tabela)
		if err != nil {
			return err
		}
		for linhas.Next() {
			var nome string
			if err := linhas.Scan(&nome); err != nil {
				linhas.Close()
				return err
			}
			existe = existe || nome == c.coluna
		}
		linhas.Close()
		if !existe {
			// Nomes fixos daqui, nunca vindos de fora: seguro montar o comando.
			if _, err := db.Exec(fmt.Sprintf("ALTER TABLE %s ADD COLUMN %s %s", c.tabela, c.coluna, c.definicao)); err != nil {
				return err
			}
		}
	}
	return nil
}

func agora() string { return time.Now().UTC().Format(time.RFC3339Nano) }

// registrar acrescenta um evento ao histórico, encadeado ao anterior.
func registrar(ctx context.Context, tx *sql.Tx, tipo string, conteudo any) error {
	var anterior string
	err := tx.QueryRowContext(ctx, `SELECT hash FROM eventos ORDER BY id DESC LIMIT 1`).Scan(&anterior)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return err
	}
	bruto, err := json.Marshal(conteudo)
	if err != nil {
		return err
	}
	momento := agora()
	_, err = tx.ExecContext(ctx, `INSERT INTO eventos (momento, tipo, dados, hash_anterior, hash) VALUES (?, ?, ?, ?, ?)`,
		momento, tipo, string(bruto), anterior, hashEvento(anterior, momento, tipo, string(bruto)))
	return err
}

func hashEvento(anterior, momento, tipo, dados string) string {
	h := sha256.New()
	for _, parte := range []string{anterior, momento, tipo, dados} {
		h.Write([]byte(parte))
		h.Write([]byte{0})
	}
	return hex.EncodeToString(h.Sum(nil))
}

// VerificarHistorico refaz a corrente de hashes e aponta o primeiro evento alterado.
func (b *Banco) VerificarHistorico(ctx context.Context) error {
	linhas, err := b.db.QueryContext(ctx, `SELECT id, momento, tipo, dados, hash_anterior, hash FROM eventos ORDER BY id`)
	if err != nil {
		return err
	}
	defer linhas.Close()
	anterior := ""
	for linhas.Next() {
		var id int64
		var momento, tipo, conteudo, hashAnterior, hash string
		if err := linhas.Scan(&id, &momento, &tipo, &conteudo, &hashAnterior, &hash); err != nil {
			return err
		}
		if hashAnterior != anterior || hash != hashEvento(anterior, momento, tipo, conteudo) {
			return fmt.Errorf("histórico alterado a partir do evento %d", id)
		}
		anterior = hash
	}
	return linhas.Err()
}

// emTransacao roda `f` e registra o evento na mesma transação.
func (b *Banco) emTransacao(ctx context.Context, f func(tx *sql.Tx) error) error {
	tx, err := b.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	if err := f(tx); err != nil {
		tx.Rollback()
		return err
	}
	return tx.Commit()
}

func traduzir(err error) error {
	if err != nil && strings.Contains(err.Error(), "UNIQUE constraint failed") {
		return ErrJaExiste
	}
	return err
}

// Validações do que vem de fora.

func nomeValido(campo, valor string, maximo int) (string, error) {
	valor = strings.TrimSpace(valor)
	if valor == "" {
		return "", ErrInvalido{campo + " não pode ficar vazio"}
	}
	if utf8.RuneCountInString(valor) > maximo {
		return "", ErrInvalido{fmt.Sprintf("%s pode ter no máximo %d caracteres", campo, maximo)}
	}
	if strings.ContainsFunc(valor, func(r rune) bool { return r < ' ' }) {
		return "", ErrInvalido{campo + " não pode ter quebras de linha nem caracteres de controle"}
	}
	return valor, nil
}

var padraoBranch = regexp.MustCompile(`^[A-Za-z0-9][A-Za-z0-9._/-]{0,199}$`)

// BranchValida aceita os nomes comuns de branch e recusa o que o git proíbe
// ou que poderia ser lido como opção de linha de comando.
func BranchValida(nome string) error {
	if !padraoBranch.MatchString(nome) || strings.Contains(nome, "..") || strings.Contains(nome, "//") ||
		strings.HasSuffix(nome, "/") || strings.HasSuffix(nome, ".lock") || strings.HasSuffix(nome, ".") {
		return ErrInvalido{fmt.Sprintf("nome de branch inválido: %q", nome)}
	}
	return nil
}

var (
	TiposProjeto = []string{"git", "pasta"}
	Locais       = []string{"pasta", "copia"}
	Papeis       = []string{"líder", "dev", "revisor", "testador"}
	Temas        = []string{"escuro", "claro", "leitura"}
	Colunas      = []string{"backlog", "trabalhando", "aguardando", "revisao", "concluido"}
	Ferramentas  = []string{"claude", "codex", "gemini", "opencode"}
	// Um agente pode ser uma das ferramentas ou um shell comum.
	TiposAgente = append([]string{"shell"}, Ferramentas...)
	ModosConta  = []string{"sistema", "separada"}
)

func umDe(campo, valor string, opcoes []string) error {
	for _, o := range opcoes {
		if o == valor {
			return nil
		}
	}
	return ErrInvalido{fmt.Sprintf("%s inválido: %q", campo, valor)}
}
