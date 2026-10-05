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
	"runtime"
	"strings"
	"time"
	"unicode/utf8"

	_ "modernc.org/sqlite"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/protecao"
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
CREATE TABLE IF NOT EXISTS anexos (` + colunasAnexos + `);
-- Histórico do que você mandou a cada agente (seta para cima na caixa de
-- mensagem). Fica fora dos eventos: um segredo colado por engano pode ser
-- apagado (decisão 0006).
CREATE TABLE IF NOT EXISTS mensagens (
	id INTEGER PRIMARY KEY,
	agente_id INTEGER NOT NULL REFERENCES agentes(id) ON DELETE CASCADE,
	texto TEXT NOT NULL,
	enviada_em TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS mensagens_por_agente ON mensagens (agente_id, id);
-- Notas da daily e da sprint, por tarefa e período.
CREATE TABLE IF NOT EXISTS notas (
	tarefa_id INTEGER NOT NULL REFERENCES tarefas(id) ON DELETE CASCADE,
	tipo TEXT NOT NULL CHECK (tipo IN ('daily', 'sprint')),
	periodo TEXT NOT NULL,
	texto TEXT NOT NULL,
	atualizada_em TEXT NOT NULL,
	PRIMARY KEY (tarefa_id, tipo, periodo)
);
`

// colunasAnexos é a definição da tabela anexos desde a versão 3 do banco
// (fotos e vídeos), com na_lousa da versão 4: uma imagem ou um vídeo posto
// numa lousa, que não aparece na linha do tempo nem nos slides. A migração
// para ela está em migrarAnexos (e na lista de colunas de migrar).
const colunasAnexos = `
	id INTEGER PRIMARY KEY,
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	tarefa_id INTEGER,
	sha256 TEXT NOT NULL,
	largura INTEGER NOT NULL,
	altura INTEGER NOT NULL,
	bytes INTEGER NOT NULL,
	origem TEXT NOT NULL CHECK (origem IN ('captura', 'colagem', 'mensagem', 'arquivo')),
	legenda TEXT NOT NULL DEFAULT '',
	criado_em TEXT NOT NULL,
	removido INTEGER NOT NULL DEFAULT 0,
	tipo TEXT NOT NULL DEFAULT 'imagem' CHECK (tipo IN ('imagem', 'video')),
	formato TEXT NOT NULL DEFAULT 'png',
	nome TEXT NOT NULL DEFAULT '',
	na_lousa INTEGER NOT NULL DEFAULT 0
`

type Banco struct {
	db *sql.DB
	// aoGravar recebe os eventos de cada transação, só depois do commit.
	aoGravar func([]Evento)
}

// Escopo diz a quem um evento pertence. Vai dentro dos dados (coberto pelo
// hash) e também em colunas indexadas, para a linha do tempo de um perfil ou
// projeto não precisar ler o histórico inteiro.
type Escopo struct {
	Perfil  int64 `json:"perfil,omitempty"`
	Projeto int64 `json:"projeto,omitempty"`
	Tarefa  int64 `json:"tarefa,omitempty"`
	Agente  int64 `json:"agente,omitempty"`
}

// Evento é uma linha do histórico.
type Evento struct {
	ID      int64           `json:"id"`
	Momento string          `json:"momento"`
	Tipo    string          `json:"tipo"`
	Dados   json.RawMessage `json:"dados"`
	Escopo  Escopo          `json:"escopo"`
}

// AoGravar liga um ouvinte aos eventos gravados. Ele é chamado depois do
// commit, na goroutine de quem gravou: uma transação desfeita não avisa nada.
func (b *Banco) AoGravar(f func([]Evento)) { b.aoGravar = f }

// Diretorio é onde ficam os dados: COLMEIA_DADOS, ou $XDG_DATA_HOME/colmeia,
// ou ~/.local/share/colmeia.
func Diretorio() (string, error) {
	if d := os.Getenv("COLMEIA_DADOS"); d != "" {
		return d, nil
	}
	// No Windows: %LOCALAPPDATA%\Colmeia\dados.
	if runtime.GOOS == "windows" {
		local, err := os.UserCacheDir()
		if err != nil {
			return "", err
		}
		return filepath.Join(local, "Colmeia", "dados"), nil
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
	if err := protecao.Diretorio(dir); err != nil {
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
	if _, err := db.Exec(esquema + esquemaPedidos + esquemaLousas + esquemaBancos); err != nil {
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
		// 1 quando a última mudança de coluna foi do núcleo (agente esperando
		// ou voltando a trabalhar); uma mudança sua zera e nunca é desfeita.
		{"tarefas", "coluna_auto", "INTEGER NOT NULL DEFAULT 0"},
		// Mostrar o aviso de segredos antes de capturar um terminal.
		{"perfis", "aviso_captura", "INTEGER NOT NULL DEFAULT 1"},
		// Mostrar o tempo dos agentes no registro (desligado por padrão).
		{"perfis", "tempo_agentes", "INTEGER NOT NULL DEFAULT 0"},
		// Workspace recolhido na barra lateral.
		{"workspaces", "recolhido", "INTEGER NOT NULL DEFAULT 0"},
		// Colunas derivadas do _escopo, fora do hash (veja preencherEscopo).
		{"eventos", "perfil_id", "INTEGER"},
		{"eventos", "projeto_id", "INTEGER"},
		{"eventos", "tarefa_id", "INTEGER"},
		{"eventos", "agente_id", "INTEGER"},
		// Imagem ou vídeo de uma lousa (versão 4): fora da linha do tempo e dos slides.
		{"anexos", "na_lousa", "INTEGER NOT NULL DEFAULT 0"},
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
	for _, indice := range []string{
		`CREATE INDEX IF NOT EXISTS eventos_por_perfil ON eventos (perfil_id, id)`,
		`CREATE INDEX IF NOT EXISTS eventos_por_tarefa ON eventos (tarefa_id)`,
		`CREATE INDEX IF NOT EXISTS anexos_por_tarefa ON anexos (tarefa_id)`,
	} {
		if _, err := db.Exec(indice); err != nil {
			return err
		}
	}
	var versao int
	if err := db.QueryRow(`PRAGMA user_version`).Scan(&versao); err != nil {
		return err
	}
	if versao < 2 {
		if err := preencherEscopo(db); err != nil {
			return fmt.Errorf("ligando eventos antigos aos perfis: %w", err)
		}
	}
	if versao < 3 {
		if err := migrarAnexos(db); err != nil {
			return fmt.Errorf("preparando os anexos para fotos e vídeos: %w", err)
		}
	}
	// Versão 4: as lousas (tabelas criadas pelo esquema) e anexos.na_lousa.
	if versao < 4 {
		if _, err := db.Exec(`PRAGMA user_version = 4`); err != nil {
			return err
		}
	}
	// Versão 5: conexões de banco por perfil e o histórico de consultas
	// (tabelas novas, criadas pelo esquema).
	if versao < 5 {
		if _, err := db.Exec(`PRAGMA user_version = 5`); err != nil {
			return err
		}
	}
	return nil
}

// migrarAnexos leva a tabela anexos à versão 3: aceita a origem "arquivo" e
// ganha tipo (imagem ou vídeo), formato e nome. O SQLite não muda um CHECK
// com ALTER, então a tabela é refeita numa transação, como a documentação
// dele recomenda (chaves estrangeiras desligadas só durante a troca e
// conferidas antes do commit). Os ids e as linhas ficam iguais.
func migrarAnexos(db *sql.DB) error {
	ctx := context.Background()
	// Uma conexão fixa: o PRAGMA vale por conexão.
	conn, err := db.Conn(ctx)
	if err != nil {
		return err
	}
	defer conn.Close()
	var temTipo int
	if err := conn.QueryRowContext(ctx, `SELECT COUNT(*) FROM pragma_table_info('anexos') WHERE name = 'tipo'`).Scan(&temTipo); err != nil {
		return err
	}
	if temTipo > 0 {
		// Banco novo: a tabela já nasceu na versão 3.
		_, err := conn.ExecContext(ctx, `PRAGMA user_version = 3`)
		return err
	}
	if _, err := conn.ExecContext(ctx, `PRAGMA foreign_keys = OFF`); err != nil {
		return err
	}
	defer conn.ExecContext(ctx, `PRAGMA foreign_keys = ON`)
	tx, err := conn.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	defer tx.Rollback()
	for _, comando := range []string{
		`CREATE TABLE anexos_nova (` + colunasAnexos + `)`,
		`INSERT INTO anexos_nova (id, perfil_id, tarefa_id, sha256, largura, altura, bytes, origem, legenda, criado_em, removido)
			SELECT id, perfil_id, tarefa_id, sha256, largura, altura, bytes, origem, legenda, criado_em, removido FROM anexos`,
		`DROP TABLE anexos`,
		`ALTER TABLE anexos_nova RENAME TO anexos`,
		`CREATE INDEX IF NOT EXISTS anexos_por_tarefa ON anexos (tarefa_id)`,
	} {
		if _, err := tx.ExecContext(ctx, comando); err != nil {
			return err
		}
	}
	linhas, err := tx.QueryContext(ctx, `PRAGMA foreign_key_check`)
	if err != nil {
		return err
	}
	problema := linhas.Next()
	linhas.Close()
	if problema {
		return errors.New("a troca da tabela de anexos quebraria referências")
	}
	if _, err := tx.ExecContext(ctx, `PRAGMA user_version = 3`); err != nil {
		return err
	}
	return tx.Commit()
}

// preencherEscopo liga os eventos gravados antes das colunas de escopo ao
// perfil, projeto, tarefa e agente deles, seguindo o próprio histórico (assim
// até o que já foi removido é ligado). É o único UPDATE feito em eventos: mexe
// só nas colunas derivadas, nunca em dados nem no hash. O que não dá para
// ligar fica sem perfil e não aparece em nenhuma linha do tempo.
func preencherEscopo(db *sql.DB) error {
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	linhas, err := tx.Query(`SELECT id, tipo, dados FROM eventos WHERE perfil_id IS NULL ORDER BY id`)
	if err != nil {
		return err
	}
	type ligacao struct {
		id int64
		e  Escopo
	}
	var ligacoes []ligacao
	perfilDoWorkspace := map[int64]int64{}
	workspaceDoProjeto := map[int64]int64{}
	projetoDaTarefa := map[int64]int64{}
	tarefaDoAgente := map[int64]int64{}
	for linhas.Next() {
		var id int64
		var tipo, conteudo string
		if err := linhas.Scan(&id, &tipo, &conteudo); err != nil {
			linhas.Close()
			return err
		}
		var d struct {
			ID          int64 `json:"id"`
			Perfil      int64 `json:"perfil"`
			PerfilID    int64 `json:"perfil_id"`
			WorkspaceID int64 `json:"workspace_id"`
			ProjetoID   int64 `json:"projeto_id"`
			TarefaID    int64 `json:"tarefa_id"`
			Projeto     any   `json:"projeto"`
			Tarefa      any   `json:"tarefa"`
			Agente      int64 `json:"agente"`
		}
		json.Unmarshal([]byte(conteudo), &d)
		numero := func(v any) int64 {
			if f, ok := v.(float64); ok {
				return int64(f)
			}
			return 0
		}
		var e Escopo
		switch tipo {
		case "perfil.criado":
			e.Perfil = d.ID
		case "perfil.tema", "perfil.contas", "workspace.recolhido":
			e.Perfil = d.Perfil
		case "workspace.criado":
			perfilDoWorkspace[d.ID] = d.PerfilID
			e.Perfil = d.PerfilID
		case "projeto.criado":
			workspaceDoProjeto[d.ID] = d.WorkspaceID
			e.Projeto = d.ID
		case "projeto.removido":
			e.Projeto = numero(d.Projeto)
		case "tarefa.criada":
			projetoDaTarefa[d.ID] = d.ProjetoID
			e.Tarefa = d.ID
		case "tarefa.atualizada", "tarefa.copia", "tarefa.removida":
			e.Tarefa = numero(d.Tarefa)
		case "agente.criado":
			tarefaDoAgente[d.ID] = d.TarefaID
			e.Agente, e.Tarefa = d.ID, d.TarefaID
		case "agente.removido":
			e.Agente = d.Agente
			e.Tarefa = tarefaDoAgente[d.Agente]
		}
		if e.Tarefa != 0 {
			e.Projeto = projetoDaTarefa[e.Tarefa]
		}
		if e.Projeto != 0 {
			e.Perfil = perfilDoWorkspace[workspaceDoProjeto[e.Projeto]]
		}
		ligacoes = append(ligacoes, ligacao{id, e})
	}
	linhas.Close()
	if err := linhas.Err(); err != nil {
		return err
	}
	for _, l := range ligacoes {
		if _, err := tx.Exec(`UPDATE eventos SET perfil_id = ?, projeto_id = ?, tarefa_id = ?, agente_id = ? WHERE id = ?`,
			nulo(l.e.Perfil), nulo(l.e.Projeto), nulo(l.e.Tarefa), nulo(l.e.Agente), l.id); err != nil {
			return err
		}
	}
	if _, err := tx.Exec(`PRAGMA user_version = 2`); err != nil {
		return err
	}
	return tx.Commit()
}

// nulo grava 0 como NULL: "sem perfil" não é o perfil 0.
func nulo(v int64) any {
	if v == 0 {
		return nil
	}
	return v
}

func agora() string { return time.Now().UTC().Format(time.RFC3339Nano) }

// transacao junta os eventos gravados nela, para avisar só depois do commit.
type transacao struct {
	*sql.Tx
	eventos []Evento
}

// registrar acrescenta um evento ao histórico, encadeado ao anterior. O
// escopo entra nos dados (dentro do hash, na chave "_escopo") e nas colunas
// indexadas.
func registrar(ctx context.Context, tx *transacao, tipo string, escopo Escopo, conteudo any) error {
	var anterior string
	err := tx.QueryRowContext(ctx, `SELECT hash FROM eventos ORDER BY id DESC LIMIT 1`).Scan(&anterior)
	if err != nil && !errors.Is(err, sql.ErrNoRows) {
		return err
	}
	bruto, err := comEscopo(conteudo, escopo)
	if err != nil {
		return err
	}
	momento := agora()
	r, err := tx.ExecContext(ctx, `INSERT INTO eventos (momento, tipo, dados, hash_anterior, hash, perfil_id, projeto_id, tarefa_id, agente_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		momento, tipo, string(bruto), anterior, hashEvento(anterior, momento, tipo, string(bruto)),
		nulo(escopo.Perfil), nulo(escopo.Projeto), nulo(escopo.Tarefa), nulo(escopo.Agente))
	if err != nil {
		return err
	}
	id, _ := r.LastInsertId()
	tx.eventos = append(tx.eventos, Evento{ID: id, Momento: momento, Tipo: tipo, Dados: bruto, Escopo: escopo})
	return nil
}

// comEscopo serializa o conteudo (sempre um objeto) com "_escopo" na frente.
func comEscopo(conteudo any, escopo Escopo) ([]byte, error) {
	bruto, err := json.Marshal(conteudo)
	if err != nil {
		return nil, err
	}
	if len(bruto) < 2 || bruto[0] != '{' {
		return nil, fmt.Errorf("evento sem objeto: %s", bruto)
	}
	e, _ := json.Marshal(escopo)
	resto := bruto[1:]
	if string(resto) != "}" {
		resto = append([]byte{','}, resto...)
	}
	return append(append([]byte(`{"_escopo":`), e...), resto...), nil
}

// Registrar grava um evento avulso (agente iniciou ou terminou, núcleo
// iniciou, anexo), fora de uma mudança das tabelas.
func (b *Banco) Registrar(ctx context.Context, tipo string, escopo Escopo, conteudo any) error {
	return b.emTransacao(ctx, func(tx *transacao) error { return registrar(ctx, tx, tipo, escopo, conteudo) })
}

func hashEvento(anterior, momento, tipo, dados string) string {
	h := sha256.New()
	for _, parte := range []string{anterior, momento, tipo, dados} {
		h.Write([]byte(parte))
		h.Write([]byte{0})
	}
	return hex.EncodeToString(h.Sum(nil))
}

// VerificarHistorico refaz a corrente de hashes e aponta o primeiro evento
// alterado. Nos eventos com "_escopo" confere também as colunas derivadas:
// mudar um evento de perfil por fora é detectado como qualquer outra mudança.
func (b *Banco) VerificarHistorico(ctx context.Context) error {
	linhas, err := b.db.QueryContext(ctx, `SELECT id, momento, tipo, dados, hash_anterior, hash,
		COALESCE(perfil_id, 0), COALESCE(projeto_id, 0), COALESCE(tarefa_id, 0), COALESCE(agente_id, 0) FROM eventos ORDER BY id`)
	if err != nil {
		return err
	}
	defer linhas.Close()
	anterior := ""
	for linhas.Next() {
		var id int64
		var momento, tipo, conteudo, hashAnterior, hash string
		var colunas Escopo
		if err := linhas.Scan(&id, &momento, &tipo, &conteudo, &hashAnterior, &hash, &colunas.Perfil, &colunas.Projeto, &colunas.Tarefa, &colunas.Agente); err != nil {
			return err
		}
		if hashAnterior != anterior || hash != hashEvento(anterior, momento, tipo, conteudo) {
			return fmt.Errorf("histórico alterado a partir do evento %d", id)
		}
		var d struct {
			Escopo *Escopo `json:"_escopo"`
		}
		if json.Unmarshal([]byte(conteudo), &d) == nil && d.Escopo != nil && *d.Escopo != colunas {
			return fmt.Errorf("histórico alterado a partir do evento %d (escopo)", id)
		}
		anterior = hash
	}
	return linhas.Err()
}

// emTransacao roda `f` numa transação; os eventos registrados nela são
// avisados depois do commit.
func (b *Banco) emTransacao(ctx context.Context, f func(tx *transacao) error) error {
	sqltx, err := b.db.BeginTx(ctx, nil)
	if err != nil {
		return err
	}
	tx := &transacao{Tx: sqltx}
	if err := f(tx); err != nil {
		sqltx.Rollback()
		return err
	}
	if err := sqltx.Commit(); err != nil {
		return err
	}
	if b.aoGravar != nil && len(tx.eventos) > 0 {
		b.aoGravar(tx.eventos)
	}
	return nil
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
	// "valor inválido para" evita errar a concordância ("coluna inválido").
	return ErrInvalido{fmt.Sprintf("valor inválido para %s: %q", campo, valor)}
}
