package dados

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"path/filepath"
	"regexp"
	"strings"
	"time"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// Origens de uma configuração de execução: escrita por você, trazida do
// IntelliJ, achada nos arquivos do projeto (package.json, Makefile...) ou
// proposta por um agente (e aceita por você).
var OrigensComando = []string{"voce", "intellij", "projeto", "agente"}

// Limites de uma configuração de execução.
const (
	MaxComandosPorProjeto = 100
	maxNomeComando        = 120
	maxTextoComando       = 4000
	maxVariaveis          = 100
)

var padraoVariavel = regexp.MustCompile(`^[A-Za-z_][A-Za-z0-9_]*$`)

// Comando é uma configuração de execução de um projeto: o "Play". O comando
// roda pelo shell, na pasta (relativa à pasta do projeto ou da tarefa), com
// as variáveis de ambiente.
type Comando struct {
	ID        int64      `json:"id"`
	ProjetoID int64      `json:"projeto_id"`
	Nome      string     `json:"nome"`
	Comando   string     `json:"comando"`
	Pasta     string     `json:"pasta"`
	Ambiente  []Variavel `json:"ambiente"`
	Origem    string     `json:"origem"`
	CriadoEm  string     `json:"criado_em"`
	// Proposta por um agente e ainda não aceita: não roda até você aceitar.
	Proposta bool `json:"proposta"`
	// Quem propôs ("Claude Code (dev)") e de que tarefa (0 sem tarefa).
	PropostaPor    string `json:"proposta_por"`
	PropostaTarefa int64  `json:"proposta_tarefa"`
	// O nome do projeto, só na lista por workspace.
	ProjetoNome string `json:"projeto_nome,omitempty"`
}

// AutorProposta é o agente que propõe uma configuração.
type AutorProposta struct {
	Agente     int64
	Ferramenta string
	Papel      string
	// Como aparece na tela ("Claude Code (dev)").
	Quem   string
	Tarefa int64
	Titulo string
}

// Variavel de ambiente de uma configuração.
type Variavel struct {
	Nome  string `json:"nome"`
	Valor string `json:"valor"`
}

// CamposComando é o que se escreve numa configuração.
type CamposComando struct {
	Nome     string     `json:"nome"`
	Comando  string     `json:"comando"`
	Pasta    string     `json:"pasta"`
	Ambiente []Variavel `json:"ambiente"`
}

// Validar confere e limpa os campos.
func (c CamposComando) Validar() (CamposComando, error) {
	c.Nome = strings.TrimSpace(c.Nome)
	if c.Nome == "" {
		return c, ErrInvalido{"dê um nome à configuração"}
	}
	if _, err := semControle("O nome", c.Nome, maxNomeComando); err != nil {
		return c, err
	}
	c.Comando = strings.TrimSpace(c.Comando)
	if c.Comando == "" {
		return c, ErrInvalido{"escreva o comando a rodar"}
	}
	if utf8.RuneCountInString(c.Comando) > maxTextoComando || !utf8.ValidString(c.Comando) || strings.ContainsRune(c.Comando, 0) {
		return c, ErrInvalido{"o comando pode ter no máximo 4000 caracteres"}
	}
	pasta, err := PastaRelativa(c.Pasta)
	if err != nil {
		return c, err
	}
	c.Pasta = pasta
	if len(c.Ambiente) > maxVariaveis {
		return c, ErrInvalido{"no máximo 100 variáveis de ambiente"}
	}
	vistas := map[string]bool{}
	for i, v := range c.Ambiente {
		v.Nome = strings.TrimSpace(v.Nome)
		if !padraoVariavel.MatchString(v.Nome) || len(v.Nome) > 200 {
			return c, ErrInvalido{"nome de variável inválido: use letras, números e _ (" + v.Nome + ")"}
		}
		if vistas[v.Nome] {
			return c, ErrInvalido{"a variável " + v.Nome + " aparece duas vezes"}
		}
		vistas[v.Nome] = true
		if _, err := semControle("O valor de "+v.Nome, v.Valor, maxTextoComando); err != nil {
			return c, err
		}
		c.Ambiente[i] = v
	}
	if c.Ambiente == nil {
		c.Ambiente = []Variavel{}
	}
	return c, nil
}

// PastaRelativa confere uma subpasta: relativa, sem sair da pasta de cima.
// Vazia ou "." é a própria pasta.
func PastaRelativa(pasta string) (string, error) {
	pasta = strings.TrimSpace(strings.ReplaceAll(pasta, `\`, "/"))
	if _, err := semControle("A pasta", pasta, 1024); err != nil {
		return "", err
	}
	if pasta == "" || pasta == "." {
		return "", nil
	}
	limpa := filepath.ToSlash(filepath.Clean(filepath.FromSlash(pasta)))
	if strings.HasPrefix(pasta, "/") || filepath.IsAbs(filepath.FromSlash(pasta)) || filepath.VolumeName(filepath.FromSlash(pasta)) != "" ||
		limpa == ".." || strings.HasPrefix(limpa, "../") {
		return "", ErrInvalido{"a pasta precisa ficar dentro do projeto (relativa a ele)"}
	}
	if limpa == "." {
		return "", nil
	}
	return limpa, nil
}

const colunasComando = `id, projeto_id, nome, comando, pasta, ambiente, origem, criado_em, proposta, proposta_por, proposta_tarefa`

func lerComando(l interface{ Scan(...any) error }) (Comando, error) {
	var c Comando
	var ambiente string
	if err := l.Scan(&c.ID, &c.ProjetoID, &c.Nome, &c.Comando, &c.Pasta, &ambiente, &c.Origem, &c.CriadoEm, &c.Proposta, &c.PropostaPor, &c.PropostaTarefa); err != nil {
		return c, err
	}
	if err := json.Unmarshal([]byte(ambiente), &c.Ambiente); err != nil || c.Ambiente == nil {
		c.Ambiente = []Variavel{}
	}
	return c, nil
}

// ListarComandos traz as configurações do projeto, pelo nome.
func (b *Banco) ListarComandos(ctx context.Context, projeto int64) ([]Comando, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT `+colunasComando+` FROM comandos WHERE projeto_id = ? ORDER BY nome COLLATE NOCASE, id`, projeto)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Comando{}
	for linhas.Next() {
		c, err := lerComando(linhas)
		if err != nil {
			return nil, err
		}
		lista = append(lista, c)
	}
	return lista, linhas.Err()
}

// ListarComandosDoWorkspace traz as configurações de todos os projetos do
// workspace, com o nome do projeto, por projeto e nome.
func (b *Banco) ListarComandosDoWorkspace(ctx context.Context, workspace int64) ([]Comando, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT c.`+strings.ReplaceAll(colunasComando, ", ", ", c.")+`, p.nome FROM comandos c JOIN projetos p ON p.id = c.projeto_id
		WHERE p.workspace_id = ? ORDER BY p.nome COLLATE NOCASE, c.nome COLLATE NOCASE, c.id`, workspace)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Comando{}
	for linhas.Next() {
		var c Comando
		var ambiente string
		if err := linhas.Scan(&c.ID, &c.ProjetoID, &c.Nome, &c.Comando, &c.Pasta, &ambiente, &c.Origem, &c.CriadoEm, &c.Proposta, &c.PropostaPor, &c.PropostaTarefa, &c.ProjetoNome); err != nil {
			return nil, err
		}
		if err := json.Unmarshal([]byte(ambiente), &c.Ambiente); err != nil || c.Ambiente == nil {
			c.Ambiente = []Variavel{}
		}
		lista = append(lista, c)
	}
	return lista, linhas.Err()
}

// ComandoPorID traz uma configuração e o perfil dono dela.
func (b *Banco) ComandoPorID(ctx context.Context, id int64) (Comando, int64, error) {
	c, err := lerComando(b.db.QueryRowContext(ctx, `SELECT `+colunasComando+` FROM comandos WHERE id = ?`, id))
	if errors.Is(err, sql.ErrNoRows) {
		return c, 0, ErrNaoEncontrado
	}
	if err != nil {
		return c, 0, err
	}
	var perfil int64
	err = b.db.QueryRowContext(ctx, `SELECT w.perfil_id FROM projetos p JOIN workspaces w ON w.id = p.workspace_id WHERE p.id = ?`, c.ProjetoID).Scan(&perfil)
	if errors.Is(err, sql.ErrNoRows) {
		return c, 0, ErrNaoEncontrado
	}
	return c, perfil, err
}

func nomeRepetido(err error) bool {
	return err != nil && strings.Contains(err.Error(), "UNIQUE constraint failed: comandos.projeto_id, comandos.nome")
}

// CriarComando grava uma configuração nova no projeto (escrita por você,
// do IntelliJ ou dos arquivos do projeto): já vale, sem proposta.
func (b *Banco) CriarComando(ctx context.Context, projeto int64, campos CamposComando, origem string) (Comando, error) {
	if err := umDe("origem", origem, OrigensComando); err != nil {
		return Comando{}, err
	}
	if origem == "agente" {
		return Comando{}, ErrInvalido{"uma configuração de agente entra como proposta"}
	}
	return b.inserirComando(ctx, projeto, campos, origem, nil)
}

// ProporComando grava a configuração que um agente propôs: fica como
// proposta (não roda) até AceitarComando, e a linha do tempo conta. Comando
// e valores com cara de segredo são recusados: o valor é você quem põe.
func (b *Banco) ProporComando(ctx context.Context, projeto int64, campos CamposComando, autor AutorProposta) (Comando, error) {
	if segredos.Parece(campos.Comando) {
		return Comando{}, ErrInvalido{"o comando parece conter um segredo; deixe o valor para o usuário preencher na Colmeia"}
	}
	for _, v := range campos.Ambiente {
		if segredos.Parece(v.Valor) {
			return Comando{}, ErrInvalido{"o valor de " + strings.TrimSpace(v.Nome) + " parece um segredo; deixe-o vazio para o usuário preencher na Colmeia"}
		}
	}
	return b.inserirComando(ctx, projeto, campos, "agente", &autor)
}

func (b *Banco) inserirComando(ctx context.Context, projeto int64, campos CamposComando, origem string, autor *AutorProposta) (Comando, error) {
	campos, err := campos.Validar()
	if err != nil {
		return Comando{}, err
	}
	var c Comando
	err = b.emTransacao(ctx, func(tx *transacao) error {
		perfil, nomeProjeto, err := donoDoProjeto(ctx, tx, projeto)
		if err != nil {
			return err
		}
		var n int
		if err := tx.QueryRowContext(ctx, `SELECT count(*) FROM comandos WHERE projeto_id = ?`, projeto).Scan(&n); err != nil {
			return err
		}
		if n >= MaxComandosPorProjeto {
			return ErrInvalido{"o projeto já tem 100 configurações de execução"}
		}
		ambiente, _ := json.Marshal(campos.Ambiente)
		agora := time.Now().UTC().Format(time.RFC3339Nano)
		c = Comando{ProjetoID: projeto, Nome: campos.Nome, Comando: campos.Comando, Pasta: campos.Pasta, Ambiente: campos.Ambiente, Origem: origem, CriadoEm: agora}
		if autor != nil {
			c.Proposta, c.PropostaPor, c.PropostaTarefa = true, autor.Quem, autor.Tarefa
		}
		r, err := tx.ExecContext(ctx, `INSERT INTO comandos (projeto_id, nome, comando, pasta, ambiente, origem, criado_em, proposta, proposta_por, proposta_tarefa) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			projeto, campos.Nome, campos.Comando, campos.Pasta, string(ambiente), origem, agora, c.Proposta, c.PropostaPor, c.PropostaTarefa)
		if nomeRepetido(err) {
			return ErrInvalido{"já existe uma configuração com o nome " + campos.Nome}
		}
		if err != nil {
			return err
		}
		c.ID, _ = r.LastInsertId()
		if autor == nil {
			return nil
		}
		escopo := Escopo{Perfil: perfil, Projeto: projeto, Tarefa: autor.Tarefa, Agente: autor.Agente}
		return registrar(ctx, tx, "comando.proposto", escopo, map[string]any{
			"comando": c.ID, "nome": c.Nome, "projeto_nome": nomeProjeto, "titulo": autor.Titulo,
			"agente": autor.Agente, "ferramenta": autor.Ferramenta, "papel": autor.Papel,
		})
	})
	return c, err
}

// AceitarComando faz valer a proposta de um agente: a configuração passa a
// rodar como qualquer outra, e a linha do tempo conta.
func (b *Banco) AceitarComando(ctx context.Context, id int64) (Comando, error) {
	var c Comando
	err := b.emTransacao(ctx, func(tx *transacao) error {
		var err error
		c, err = lerComando(tx.QueryRowContext(ctx, `SELECT `+colunasComando+` FROM comandos WHERE id = ?`, id))
		if errors.Is(err, sql.ErrNoRows) {
			return ErrNaoEncontrado
		}
		if err != nil {
			return err
		}
		if !c.Proposta {
			return ErrInvalido{"esta configuração não é uma proposta"}
		}
		perfil, nomeProjeto, err := donoDoProjeto(ctx, tx, c.ProjetoID)
		if err != nil {
			return err
		}
		if _, err := tx.ExecContext(ctx, `UPDATE comandos SET proposta = 0 WHERE id = ?`, id); err != nil {
			return err
		}
		c.Proposta = false
		escopo := Escopo{Perfil: perfil, Projeto: c.ProjetoID, Tarefa: c.PropostaTarefa}
		return registrar(ctx, tx, "comando.aceito", escopo, map[string]any{"comando": c.ID, "nome": c.Nome, "projeto_nome": nomeProjeto, "proposta_por": c.PropostaPor})
	})
	return c, err
}

// EditarComando troca os campos de uma configuração.
func (b *Banco) EditarComando(ctx context.Context, id int64, campos CamposComando) (Comando, error) {
	campos, err := campos.Validar()
	if err != nil {
		return Comando{}, err
	}
	ambiente, _ := json.Marshal(campos.Ambiente)
	r, err := b.db.ExecContext(ctx, `UPDATE comandos SET nome = ?, comando = ?, pasta = ?, ambiente = ? WHERE id = ?`,
		campos.Nome, campos.Comando, campos.Pasta, string(ambiente), id)
	if nomeRepetido(err) {
		return Comando{}, ErrInvalido{"já existe uma configuração com o nome " + campos.Nome}
	}
	if err != nil {
		return Comando{}, err
	}
	if n, _ := r.RowsAffected(); n == 0 {
		return Comando{}, ErrNaoEncontrado
	}
	c, _, err := b.ComandoPorID(ctx, id)
	return c, err
}

// RemoverComando apaga uma configuração.
func (b *Banco) RemoverComando(ctx context.Context, id int64) error {
	r, err := b.db.ExecContext(ctx, `DELETE FROM comandos WHERE id = ?`, id)
	if err != nil {
		return err
	}
	if n, _ := r.RowsAffected(); n == 0 {
		return ErrNaoEncontrado
	}
	return nil
}
