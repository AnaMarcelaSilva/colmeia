package dados

import (
	"context"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"math"
	"slices"
	"sort"
	"strings"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/segredos"
)

// Lousa: o quadro livre de um workspace ou de uma tarefa, onde você solta
// notas, textos, trechos de código, imagens, vídeos, cartões de tarefa e as
// ligações entre eles. Mexer na lousa (mover, editar, apagar) fica fora da
// corrente de eventos: cada arrasto seria uma linha imutável para sempre e o
// texto ficaria preso no hash (decisão 0008). Só o que o agente acrescenta
// ou apaga entra no histórico, como "lousa.agente", sem texto.

// Limites da lousa, conferidos aqui (a tela só repete).
const (
	MaxElementosLousa = 2000
	MaxTextoLousa     = 8000
	MaxTituloLousa    = 120
	MaxOperacoesLousa = 500
	// MaxDoAgente: elementos que o agente acrescenta numa chamada.
	MaxDoAgente = 50
	// Coordenadas e tamanhos em unidades do quadro (1 = 1 px a 100%).
	MaxCoordenadaLousa = 1e6
	MinLadoLousa       = 16
	MaxLadoLousa       = 6000
	maxRef             = 64
)

var (
	TiposElemento = []string{"nota", "texto", "codigo", "imagem", "video", "tarefa", "ligacao"}
	CoresLousa    = []string{"amarelo", "azul", "verde", "rosa", "lilas", "cinza"}
	// Os tipos de texto trocam entre si ("colar um log como nota e virar código").
	tiposDeTexto = []string{"nota", "texto", "codigo"}
	// O agente acrescenta só estes.
	tiposDoAgente = []string{"nota", "texto", "codigo", "ligacao", "imagem"}
)

// colunasTabelaLousas e colunasTabelaElementos são as definições das duas
// tabelas desde a versão 15 do banco (a lousa da reunião: donos dia e
// sprint, autor montador e a tarefa de cada peça montada); a migração refaz
// as tabelas com elas.
const colunasTabelaLousas = `
	id INTEGER PRIMARY KEY,
	perfil_id INTEGER NOT NULL REFERENCES perfis(id) ON DELETE CASCADE,
	workspace_id INTEGER UNIQUE REFERENCES workspaces(id) ON DELETE CASCADE,
	tarefa_id INTEGER UNIQUE REFERENCES tarefas(id) ON DELETE CASCADE,
	dia TEXT,
	sprint_id INTEGER UNIQUE REFERENCES sprints(id) ON DELETE CASCADE,
	criada_em TEXT NOT NULL,
	UNIQUE (perfil_id, dia),
	CHECK ((workspace_id IS NOT NULL) + (tarefa_id IS NOT NULL) + (dia IS NOT NULL) + (sprint_id IS NOT NULL) = 1)`

const colunasTabelaElementos = `
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	lousa_id INTEGER NOT NULL REFERENCES lousas(id) ON DELETE CASCADE,
	tipo TEXT NOT NULL CHECK (tipo IN ('nota', 'texto', 'codigo', 'imagem', 'video', 'tarefa', 'ligacao')),
	x REAL NOT NULL,
	y REAL NOT NULL,
	largura REAL NOT NULL,
	altura REAL NOT NULL,
	z INTEGER NOT NULL,
	cor TEXT NOT NULL DEFAULT 'amarelo' CHECK (cor IN ('amarelo', 'azul', 'verde', 'rosa', 'lilas', 'cinza')),
	titulo TEXT NOT NULL DEFAULT '',
	texto TEXT NOT NULL DEFAULT '',
	anexo_id INTEGER REFERENCES anexos(id),
	tarefa_ref INTEGER REFERENCES tarefas(id) ON DELETE SET NULL,
	de_id INTEGER REFERENCES lousa_elementos(id) ON DELETE CASCADE,
	para_id INTEGER REFERENCES lousa_elementos(id) ON DELETE CASCADE,
	autor TEXT NOT NULL CHECK (autor IN ('voce', 'agente', 'montador')),
	agente_id INTEGER,
	montagem_de INTEGER,
	versao INTEGER NOT NULL DEFAULT 1,
	atualizado_em TEXT NOT NULL`

const esquemaLousas = `
CREATE TABLE IF NOT EXISTS lousas (` + colunasTabelaLousas + `
);
CREATE TABLE IF NOT EXISTS lousa_elementos (` + colunasTabelaElementos + `
);
CREATE INDEX IF NOT EXISTS lousa_elementos_por_lousa ON lousa_elementos (lousa_id, z);
CREATE INDEX IF NOT EXISTS lousa_ligacoes_de ON lousa_elementos (de_id);
CREATE INDEX IF NOT EXISTS lousa_ligacoes_para ON lousa_elementos (para_id);
CREATE INDEX IF NOT EXISTS lousa_cartoes_de_tarefa ON lousa_elementos (tarefa_ref);
`

// DonoLousa: um workspace, uma tarefa, a daily de um dia do perfil ou uma
// sprint (só um dos quatro). PerfilID só acompanha o dia.
type DonoLousa struct {
	WorkspaceID int64  `json:"workspace_id,omitempty"`
	TarefaID    int64  `json:"tarefa_id,omitempty"`
	PerfilID    int64  `json:"perfil_id,omitempty"`
	Dia         string `json:"dia,omitempty"`
	SprintID    int64  `json:"sprint_id,omitempty"`
}

// DaReuniao diz se a lousa é a de uma daily ou de uma sprint.
func (d DonoLousa) DaReuniao() bool { return d.Dia != "" || d.SprintID != 0 }

type Lousa struct {
	ID       int64     `json:"id"`
	PerfilID int64     `json:"-"`
	Dono     DonoLousa `json:"dono"`
}

// AnexoDoElemento: o que a tela precisa do anexo de uma imagem ou de um vídeo
// (a proporção, o nome e o tamanho do cartão de vídeo).
type AnexoDoElemento struct {
	Nome    string `json:"nome,omitempty"`
	Bytes   int    `json:"bytes"`
	Largura int    `json:"largura"`
	Altura  int    `json:"altura"`
}

// Elemento é um item da lousa. Posições e tamanhos em unidades do quadro.
type Elemento struct {
	ID        int64            `json:"id"`
	LousaID   int64            `json:"lousa_id"`
	Tipo      string           `json:"tipo"`
	X         float64          `json:"x"`
	Y         float64          `json:"y"`
	Largura   float64          `json:"largura"`
	Altura    float64          `json:"altura"`
	Z         int64            `json:"z"`
	Cor       string           `json:"cor"`
	Titulo    string           `json:"titulo"`
	Texto     string           `json:"texto"`
	AnexoID   int64            `json:"anexo_id,omitempty"`
	Anexo     *AnexoDoElemento `json:"anexo,omitempty"`
	TarefaRef int64            `json:"tarefa_ref,omitempty"`
	De        int64            `json:"de,omitempty"`
	Para      int64            `json:"para,omitempty"`
	// Autor: "voce", "agente" (com o agente que acrescentou) ou "montador"
	// (o núcleo, na lousa da reunião; MontagemDe é a tarefa da peça).
	Autor        string `json:"autor"`
	AgenteID     int64  `json:"agente_id,omitempty"`
	MontagemDe   int64  `json:"montagem_de,omitempty"`
	Versao       int64  `json:"versao"`
	AtualizadoEm string `json:"atualizado_em"`
}

// RefElemento aponta para um elemento: um id que já existe (número no JSON)
// ou o "ref" de um elemento criado no mesmo lote (texto no JSON).
type RefElemento struct {
	ID  int64
	Ref string
}

func (r *RefElemento) UnmarshalJSON(b []byte) error {
	if len(b) > 0 && b[0] == '"' {
		if err := json.Unmarshal(b, &r.Ref); err != nil {
			return err
		}
		if r.Ref == "" {
			return errors.New("referência vazia")
		}
		return nil
	}
	return json.Unmarshal(b, &r.ID)
}

func (r RefElemento) MarshalJSON() ([]byte, error) {
	if r.Ref != "" {
		return json.Marshal(r.Ref)
	}
	return json.Marshal(r.ID)
}

// NovoElemento é um elemento a criar. Sem x e y (só o agente pode omitir),
// o núcleo posiciona; sem largura e altura, estima pelo texto.
type NovoElemento struct {
	Tipo      string       `json:"tipo"`
	X         *float64     `json:"x,omitempty"`
	Y         *float64     `json:"y,omitempty"`
	Largura   *float64     `json:"largura,omitempty"`
	Altura    *float64     `json:"altura,omitempty"`
	Z         *int64       `json:"z,omitempty"`
	Cor       string       `json:"cor,omitempty"`
	Titulo    string       `json:"titulo,omitempty"`
	Texto     string       `json:"texto,omitempty"`
	AnexoID   int64        `json:"anexo_id,omitempty"`
	TarefaRef int64        `json:"tarefa_ref,omitempty"`
	De        *RefElemento `json:"de,omitempty"`
	Para      *RefElemento `json:"para,omitempty"`
	// MontagemDe: a tarefa da peça, só pelo montador (nunca pela tela).
	MontagemDe int64 `json:"-"`
}

// CamposElemento: só os campos enviados mudam.
type CamposElemento struct {
	X       *float64 `json:"x,omitempty"`
	Y       *float64 `json:"y,omitempty"`
	Largura *float64 `json:"largura,omitempty"`
	Altura  *float64 `json:"altura,omitempty"`
	Z       *int64   `json:"z,omitempty"`
	Cor     *string  `json:"cor,omitempty"`
	Titulo  *string  `json:"titulo,omitempty"`
	Texto   *string  `json:"texto,omitempty"`
	// Tipo troca só entre nota, texto e código.
	Tipo *string `json:"tipo,omitempty"`
	// De e Para trocam as pontas de uma ligação ("Inverter").
	De   *int64 `json:"de,omitempty"`
	Para *int64 `json:"para,omitempty"`
}

// Operacao de um lote: criar, alterar ou remover.
type Operacao struct {
	Op       string          `json:"op"`
	Ref      string          `json:"ref,omitempty"`
	ID       int64           `json:"id,omitempty"`
	Versao   int64           `json:"versao,omitempty"`
	Elemento *NovoElemento   `json:"elemento,omitempty"`
	Campos   *CamposElemento `json:"campos,omitempty"`
}

// ResultadoLousa: os elementos como ficaram, os removidos (inclusive as
// ligações levadas junto) e o id de cada "ref" criado.
type ResultadoLousa struct {
	Lousa     Lousa            `json:"-"`
	Elementos []Elemento       `json:"elementos"`
	Removidos []int64          `json:"removidos"`
	Refs      map[string]int64 `json:"refs"`
}

// ErrLousaMudou: alguma versão do lote não bate (outra tela ou o agente
// mexeu antes). Nada foi gravado; Elementos é o estado atual dos que mudaram
// e Removidos, os que já não existem.
type ErrLousaMudou struct {
	Elementos []Elemento
	Removidos []int64
}

func (e ErrLousaMudou) Error() string { return "a lousa mudou em outra tela" }

// autorLousa: quem grava. Agente 0 é você.
type autorLousa struct {
	agente int64
	// soDaTarefa: o agente só usa anexos da própria tarefa.
	soDaTarefa int64
	// montador: o núcleo montando a lousa da reunião.
	montador bool
}

func (a autorLousa) nome() string {
	switch {
	case a.montador:
		return "montador"
	case a.agente != 0:
		return "agente"
	}
	return "voce"
}

// Leitura

const colunasElemento = `e.id, e.lousa_id, e.tipo, e.x, e.y, e.largura, e.altura, e.z, e.cor, e.titulo, e.texto,
	COALESCE(e.anexo_id, 0), COALESCE(e.tarefa_ref, 0), COALESCE(e.de_id, 0), COALESCE(e.para_id, 0), e.autor, COALESCE(e.agente_id, 0), COALESCE(e.montagem_de, 0),
	e.versao, e.atualizado_em, COALESCE(a.nome, ''), COALESCE(a.bytes, 0), COALESCE(a.largura, 0), COALESCE(a.altura, 0)`

const deElementos = ` FROM lousa_elementos e LEFT JOIN anexos a ON a.id = e.anexo_id `

type consultavel interface {
	QueryContext(ctx context.Context, query string, args ...any) (*sql.Rows, error)
}

func lerElementos(ctx context.Context, c consultavel, onde string, args ...any) ([]Elemento, error) {
	linhas, err := c.QueryContext(ctx, `SELECT `+colunasElemento+deElementos+onde, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	lista := []Elemento{}
	for linhas.Next() {
		var e Elemento
		var info AnexoDoElemento
		if err := linhas.Scan(&e.ID, &e.LousaID, &e.Tipo, &e.X, &e.Y, &e.Largura, &e.Altura, &e.Z, &e.Cor, &e.Titulo, &e.Texto,
			&e.AnexoID, &e.TarefaRef, &e.De, &e.Para, &e.Autor, &e.AgenteID, &e.MontagemDe, &e.Versao, &e.AtualizadoEm,
			&info.Nome, &info.Bytes, &info.Largura, &info.Altura); err != nil {
			return nil, err
		}
		if e.AnexoID != 0 {
			e.Anexo = &info
		}
		lista = append(lista, e)
	}
	return lista, linhas.Err()
}

// elementosPorID lê os elementos dados, na ordem de desenho.
func elementosPorID(ctx context.Context, c consultavel, ids []int64) ([]Elemento, error) {
	if len(ids) == 0 {
		return []Elemento{}, nil
	}
	args := make([]any, len(ids))
	for i, id := range ids {
		args[i] = id
	}
	return lerElementos(ctx, c, `WHERE e.id IN (?`+strings.Repeat(`, ?`, len(ids)-1)+`) ORDER BY e.z, e.id`, args...)
}

// ElementosPorID lê elementos pelo id (os que não existem mais ficam de fora).
func (b *Banco) ElementosPorID(ctx context.Context, ids []int64) ([]Elemento, error) {
	return elementosPorID(ctx, b.db, ids)
}

func lerLousa(ctx context.Context, c interface {
	QueryRowContext(ctx context.Context, query string, args ...any) *sql.Row
}, id int64) (Lousa, error) {
	var l Lousa
	var ws, tarefa, sprint sql.NullInt64
	var dia sql.NullString
	err := c.QueryRowContext(ctx, `SELECT id, perfil_id, workspace_id, tarefa_id, dia, sprint_id FROM lousas WHERE id = ?`, id).Scan(&l.ID, &l.PerfilID, &ws, &tarefa, &dia, &sprint)
	if errors.Is(err, sql.ErrNoRows) {
		return l, ErrNaoEncontrado
	}
	l.Dono = DonoLousa{WorkspaceID: ws.Int64, TarefaID: tarefa.Int64, Dia: dia.String, SprintID: sprint.Int64}
	if l.Dono.Dia != "" {
		l.Dono.PerfilID = l.PerfilID
	}
	return l, err
}

// Lousa lê a lousa e os elementos dela, na ordem de desenho.
func (b *Banco) Lousa(ctx context.Context, id int64) (Lousa, []Elemento, error) {
	l, err := lerLousa(ctx, b.db, id)
	if err != nil {
		return l, nil, err
	}
	elementos, err := lerElementos(ctx, b.db, `WHERE e.lousa_id = ? ORDER BY e.z, e.id`, id)
	return l, elementos, err
}

// AbrirLousa traz a lousa do workspace ou da tarefa, criando-a se ainda não
// existe. Criar não gera evento: uma lousa vazia não é novidade.
func (b *Banco) AbrirLousa(ctx context.Context, dono DonoLousa) (Lousa, []Elemento, error) {
	var id int64
	err := b.emTransacao(ctx, func(tx *transacao) error {
		var err error
		id, _, err = garantirLousa(ctx, tx, dono)
		return err
	})
	if err != nil {
		return Lousa{}, nil, err
	}
	return b.Lousa(ctx, id)
}

// garantirLousa acha (ou cria) a lousa do dono e diz o perfil dele.
func garantirLousa(ctx context.Context, tx *transacao, dono DonoLousa) (int64, int64, error) {
	perfil, err := perfilDoDono(ctx, tx, dono)
	if err != nil {
		return 0, 0, err
	}
	if _, err := tx.ExecContext(ctx, `INSERT INTO lousas (perfil_id, workspace_id, tarefa_id, dia, sprint_id, criada_em) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT DO NOTHING`,
		perfil, nulo(dono.WorkspaceID), nulo(dono.TarefaID), textoNulo(dono.Dia), nulo(dono.SprintID), agora()); err != nil {
		return 0, 0, err
	}
	id, err := lousaDoDono(ctx, tx, dono, perfil)
	return id, perfil, err
}

// perfilDoDono confere que o dono é um só e existe, e diz o perfil dele.
func perfilDoDono(ctx context.Context, tx *transacao, dono DonoLousa) (int64, error) {
	donos := 0
	for _, tem := range []bool{dono.WorkspaceID != 0, dono.TarefaID != 0, dono.Dia != "", dono.SprintID != 0} {
		if tem {
			donos++
		}
	}
	if donos != 1 {
		return 0, ErrInvalido{"a lousa é de um workspace, de uma tarefa, de uma daily ou de uma sprint"}
	}
	var perfil int64
	var err error
	switch {
	case dono.WorkspaceID != 0:
		err = tx.QueryRowContext(ctx, `SELECT perfil_id FROM workspaces WHERE id = ?`, dono.WorkspaceID).Scan(&perfil)
	case dono.TarefaID != 0:
		var projeto int64
		if err = tx.QueryRowContext(ctx, `SELECT projeto_id FROM tarefas WHERE id = ?`, dono.TarefaID).Scan(&projeto); err == nil {
			perfil, _, err = donoDoProjeto(ctx, tx, projeto)
		}
	case dono.Dia != "":
		if !dataReal(dono.Dia) {
			return 0, ErrInvalido{"o dia da daily precisa ser AAAA-MM-DD"}
		}
		if dono.PerfilID == 0 {
			return 0, ErrInvalido{"a lousa da daily precisa do perfil"}
		}
		err = tx.QueryRowContext(ctx, `SELECT id FROM perfis WHERE id = ?`, dono.PerfilID).Scan(&perfil)
	default:
		err = tx.QueryRowContext(ctx, `SELECT perfil_id FROM sprints WHERE id = ?`, dono.SprintID).Scan(&perfil)
	}
	if errors.Is(err, sql.ErrNoRows) {
		return 0, ErrNaoEncontrado
	}
	return perfil, err
}

// lousaDoDono acha a lousa de um dono (0 se não existe).
func lousaDoDono(ctx context.Context, tx *transacao, dono DonoLousa, perfil int64) (int64, error) {
	var id int64
	var err error
	switch {
	case dono.WorkspaceID != 0:
		err = tx.QueryRowContext(ctx, `SELECT id FROM lousas WHERE workspace_id = ?`, dono.WorkspaceID).Scan(&id)
	case dono.TarefaID != 0:
		err = tx.QueryRowContext(ctx, `SELECT id FROM lousas WHERE tarefa_id = ?`, dono.TarefaID).Scan(&id)
	case dono.Dia != "":
		err = tx.QueryRowContext(ctx, `SELECT id FROM lousas WHERE perfil_id = ? AND dia = ?`, perfil, dono.Dia).Scan(&id)
	default:
		err = tx.QueryRowContext(ctx, `SELECT id FROM lousas WHERE sprint_id = ?`, dono.SprintID).Scan(&id)
	}
	if errors.Is(err, sql.ErrNoRows) {
		return 0, nil
	}
	return id, err
}

// LousaExistente acha a lousa de um dono sem criar (ok false se não há).
func (b *Banco) LousaExistente(ctx context.Context, dono DonoLousa) (Lousa, bool, error) {
	var id int64
	err := b.emTransacao(ctx, func(tx *transacao) error {
		perfil, err := perfilDoDono(ctx, tx, dono)
		if err != nil {
			return err
		}
		id, err = lousaDoDono(ctx, tx, dono, perfil)
		return err
	})
	if err != nil || id == 0 {
		return Lousa{}, false, err
	}
	l, err := lerLousa(ctx, b.db, id)
	return l, err == nil, err
}

func textoNulo(v string) any {
	if v == "" {
		return nil
	}
	return v
}

// ResumoLousa: a lousa de uma tarefa e quantos elementos ela tem.
type ResumoLousa struct {
	ID        int64 `json:"id"`
	Elementos int   `json:"elementos"`
}

// LousasDasTarefas traz, por tarefa, a lousa que tem elementos.
func (b *Banco) LousasDasTarefas(ctx context.Context, tarefas []int64) (map[int64]ResumoLousa, error) {
	resumo := map[int64]ResumoLousa{}
	if len(tarefas) == 0 {
		return resumo, nil
	}
	args := make([]any, len(tarefas))
	for i, t := range tarefas {
		args[i] = t
	}
	linhas, err := b.db.QueryContext(ctx, `SELECT l.tarefa_id, l.id, COUNT(e.id) FROM lousas l JOIN lousa_elementos e ON e.lousa_id = l.id
		WHERE l.tarefa_id IN (?`+strings.Repeat(`, ?`, len(tarefas)-1)+`) GROUP BY l.id`, args...)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	for linhas.Next() {
		var tarefa int64
		var r ResumoLousa
		if err := linhas.Scan(&tarefa, &r.ID, &r.Elementos); err != nil {
			return nil, err
		}
		resumo[tarefa] = r
	}
	return resumo, linhas.Err()
}

// Validação

func numeroFinito(campo string, v float64) error {
	if math.IsNaN(v) || math.IsInf(v, 0) {
		return ErrInvalido{campo + " precisa ser um número"}
	}
	return nil
}

func posicaoValida(x, y float64) error {
	for _, c := range []struct {
		nome string
		v    float64
	}{{"x", x}, {"y", y}} {
		if err := numeroFinito(c.nome, c.v); err != nil {
			return err
		}
		if math.Abs(c.v) > MaxCoordenadaLousa {
			return ErrInvalido{"o item está longe demais do centro da lousa"}
		}
	}
	return nil
}

func tamanhoValido(largura, altura float64) error {
	for _, v := range []float64{largura, altura} {
		if err := numeroFinito("o tamanho", v); err != nil {
			return err
		}
		if v < MinLadoLousa || v > MaxLadoLousa {
			return ErrInvalido{fmt.Sprintf("largura e altura vão de %d a %d", MinLadoLousa, MaxLadoLousa)}
		}
	}
	return nil
}

// textoDaLousa confere um texto de usuário. As mensagens nunca repetem o
// texto: elas podem parar num log.
func textoDaLousa(campo, texto string, maximo int, linhas bool) (string, error) {
	texto = strings.ReplaceAll(texto, "\r\n", "\n")
	texto = strings.ReplaceAll(texto, "\r", "\n")
	if !utf8.ValidString(texto) {
		return "", ErrInvalido{campo + " tem caracteres inválidos"}
	}
	if strings.ContainsFunc(texto, func(r rune) bool {
		return (r < ' ' && !(linhas && (r == '\n' || r == '\t'))) || r == 0x7f
	}) {
		return "", ErrInvalido{campo + " tem caracteres de controle"}
	}
	if utf8.RuneCountInString(texto) > maximo {
		return "", ErrInvalido{fmt.Sprintf("%s pode ter no máximo %d caracteres", campo, maximo)}
	}
	// A lousa vai para o telão: nada que pareça senha ou chave (decisão 0006).
	if segredos.Parece(texto) {
		return "", ErrInvalido{"Parece ter senha ou chave; não salvei este texto"}
	}
	return texto, nil
}

func umDosTipos(tipo string, tipos []string) bool {
	for _, t := range tipos {
		if t == tipo {
			return true
		}
	}
	return false
}

// conferirAnexo: o anexo é do perfil, não foi removido e é do tipo do elemento.
func conferirAnexo(ctx context.Context, tx *transacao, id, perfil int64, tipo string, autor autorLousa) error {
	var dono, tarefa int64
	var removido bool
	var tipoAnexo string
	err := tx.QueryRowContext(ctx, `SELECT perfil_id, COALESCE(tarefa_id, 0), removido, tipo FROM anexos WHERE id = ?`, id).Scan(&dono, &tarefa, &removido, &tipoAnexo)
	if errors.Is(err, sql.ErrNoRows) || (err == nil && (dono != perfil || removido)) {
		return ErrInvalido{fmt.Sprintf("o anexo %d não existe neste perfil", id)}
	}
	if err != nil {
		return err
	}
	if autor.soDaTarefa != 0 && tarefa != autor.soDaTarefa {
		return ErrInvalido{fmt.Sprintf("o anexo %d não é desta tarefa", id)}
	}
	if tipoAnexo != tipo {
		if tipo == "video" {
			return ErrInvalido{fmt.Sprintf("o anexo %d não é um vídeo", id)}
		}
		return ErrInvalido{fmt.Sprintf("o anexo %d não é uma imagem", id)}
	}
	return nil
}

func conferirTarefa(ctx context.Context, tx *transacao, id, perfil int64) error {
	var projeto int64
	err := tx.QueryRowContext(ctx, `SELECT projeto_id FROM tarefas WHERE id = ?`, id).Scan(&projeto)
	if errors.Is(err, sql.ErrNoRows) {
		return ErrInvalido{fmt.Sprintf("a tarefa %d não existe", id)}
	}
	if err != nil {
		return err
	}
	dono, _, err := donoDoProjeto(ctx, tx, projeto)
	if err != nil {
		return err
	}
	if dono != perfil {
		return ErrInvalido{fmt.Sprintf("a tarefa %d não existe", id)}
	}
	return nil
}

// ponta confere a ponta de uma ligação: um elemento da mesma lousa que não
// é uma ligação.
func ponta(ctx context.Context, tx *transacao, lousa, id int64) error {
	var tipo string
	err := tx.QueryRowContext(ctx, `SELECT tipo FROM lousa_elementos WHERE id = ? AND lousa_id = ?`, id, lousa).Scan(&tipo)
	if errors.Is(err, sql.ErrNoRows) {
		return ErrInvalido{fmt.Sprintf("a ligação aponta para o item %d, que não está nesta lousa", id)}
	}
	if err != nil {
		return err
	}
	if tipo == "ligacao" {
		return ErrInvalido{"uma ligação não liga outra ligação"}
	}
	return nil
}

// Gravação

// GravarLousa aplica um lote de operações da tela, tudo ou nada.
func (b *Banco) GravarLousa(ctx context.Context, lousa int64, ops []Operacao) (ResultadoLousa, error) {
	var r ResultadoLousa
	err := b.emTransacao(ctx, func(tx *transacao) error {
		l, err := lerLousa(ctx, tx, lousa)
		if err != nil {
			return err
		}
		r, err = aplicarLote(ctx, tx, l, ops, autorLousa{})
		return err
	})
	return r, err
}

// aplicarLote confere as versões antes de tudo (se uma não bate, nada é
// gravado) e aplica as operações em ordem.
func aplicarLote(ctx context.Context, tx *transacao, l Lousa, ops []Operacao, autor autorLousa) (ResultadoLousa, error) {
	r := ResultadoLousa{Lousa: l, Elementos: []Elemento{}, Removidos: []int64{}, Refs: map[string]int64{}}
	if len(ops) == 0 {
		return r, ErrInvalido{"o lote está vazio"}
	}
	if len(ops) > MaxOperacoesLousa {
		return r, ErrInvalido{fmt.Sprintf("um lote pode ter no máximo %d operações", MaxOperacoesLousa)}
	}
	// Versões: todas conferidas antes de gravar qualquer coisa.
	vistos := map[int64]bool{}
	var mudaram, sumiram []int64
	for _, op := range ops {
		if op.Op != "alterar" && op.Op != "remover" {
			continue
		}
		if op.ID <= 0 {
			return r, ErrInvalido{"falta o id do item"}
		}
		if vistos[op.ID] {
			return r, ErrInvalido{"o mesmo item aparece duas vezes no lote"}
		}
		vistos[op.ID] = true
		var versao int64
		err := tx.QueryRowContext(ctx, `SELECT versao FROM lousa_elementos WHERE id = ? AND lousa_id = ?`, op.ID, l.ID).Scan(&versao)
		switch {
		case errors.Is(err, sql.ErrNoRows):
			sumiram = append(sumiram, op.ID)
		case err != nil:
			return r, err
		case versao != op.Versao:
			mudaram = append(mudaram, op.ID)
		}
	}
	if len(mudaram) > 0 || len(sumiram) > 0 {
		atuais, err := elementosPorID(ctx, tx, mudaram)
		if err != nil {
			return r, err
		}
		if sumiram == nil {
			sumiram = []int64{}
		}
		return r, ErrLousaMudou{Elementos: atuais, Removidos: sumiram}
	}

	var proximoZ int64
	if err := tx.QueryRowContext(ctx, `SELECT COALESCE(MAX(z), 0) FROM lousa_elementos WHERE lousa_id = ?`, l.ID).Scan(&proximoZ); err != nil {
		return r, err
	}
	tocados := []int64{}
	removidos := map[int64]bool{}
	momento := agora()
	for _, op := range ops {
		switch op.Op {
		case "criar":
			if op.Elemento == nil {
				return r, ErrInvalido{"falta o elemento a criar"}
			}
			if op.Ref != "" {
				if utf8.RuneCountInString(op.Ref) > maxRef {
					return r, ErrInvalido{"ref longo demais"}
				}
				if _, repetido := r.Refs[op.Ref]; repetido {
					return r, ErrInvalido{"o mesmo ref aparece duas vezes no lote"}
				}
			}
			proximoZ++
			id, err := criarElemento(ctx, tx, l, *op.Elemento, r.Refs, removidos, proximoZ, autor, momento)
			if err != nil {
				return r, err
			}
			if op.Ref != "" {
				r.Refs[op.Ref] = id
			}
			tocados = append(tocados, id)
		case "alterar":
			if autor.agente != 0 {
				return r, ErrInvalido{"o agente não altera itens da lousa: acrescenta e apaga"}
			}
			if op.Campos == nil {
				return r, ErrInvalido{"faltam os campos a alterar"}
			}
			if err := alterarElemento(ctx, tx, l, op.ID, *op.Campos, momento); err != nil {
				return r, err
			}
			tocados = append(tocados, op.ID)
		case "remover":
			// As ligações do item saem junto (em cascata): a tela fica sabendo.
			linhas, err := tx.QueryContext(ctx, `SELECT id FROM lousa_elementos WHERE de_id = ? OR para_id = ?`, op.ID, op.ID)
			if err != nil {
				return r, err
			}
			for linhas.Next() {
				var id int64
				if err := linhas.Scan(&id); err != nil {
					linhas.Close()
					return r, err
				}
				removidos[id] = true
			}
			linhas.Close()
			if _, err := tx.ExecContext(ctx, `DELETE FROM lousa_elementos WHERE id = ?`, op.ID); err != nil {
				return r, err
			}
			removidos[op.ID] = true
		default:
			return r, ErrInvalido{"operação desconhecida (use criar, alterar ou remover)"}
		}
	}
	var total int
	if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM lousa_elementos WHERE lousa_id = ?`, l.ID).Scan(&total); err != nil {
		return r, err
	}
	if total > MaxElementosLousa {
		return r, ErrInvalido{fmt.Sprintf("A lousa chegou ao limite de %d itens", MaxElementosLousa)}
	}
	vivos := []int64{}
	for _, id := range tocados {
		if !removidos[id] {
			vivos = append(vivos, id)
		}
	}
	elementos, err := elementosPorID(ctx, tx, vivos)
	if err != nil {
		return r, err
	}
	r.Elementos = elementos
	for id := range removidos {
		r.Removidos = append(r.Removidos, id)
	}
	sort.Slice(r.Removidos, func(i, j int) bool { return r.Removidos[i] < r.Removidos[j] })
	return r, nil
}

// resolver troca um ref do lote pelo id criado.
func resolver(ref *RefElemento, refs map[string]int64) (int64, error) {
	if ref == nil {
		return 0, nil
	}
	if ref.Ref != "" {
		id, ok := refs[ref.Ref]
		if !ok {
			return 0, ErrInvalido{"a ligação aponta para um ref que não foi criado antes dela no lote"}
		}
		return id, nil
	}
	return ref.ID, nil
}

func criarElemento(ctx context.Context, tx *transacao, l Lousa, n NovoElemento, refs map[string]int64, removidos map[int64]bool, z int64, autor autorLousa, momento string) (int64, error) {
	if !umDosTipos(n.Tipo, TiposElemento) {
		return 0, ErrInvalido{"tipo de item desconhecido"}
	}
	if autor.agente != 0 && !umDosTipos(n.Tipo, tiposDoAgente) {
		return 0, ErrInvalido{"o agente acrescenta só notas, textos, código, ligações e imagens da tarefa"}
	}
	cor := n.Cor
	if cor == "" {
		cor = "amarelo"
	}
	if !umDosTipos(cor, CoresLousa) {
		return 0, ErrInvalido{"cor desconhecida (amarelo, azul, verde, rosa, lilas ou cinza)"}
	}
	var x, y, largura, altura float64
	if n.Tipo == "ligacao" {
		// A ligação é desenhada entre as pontas: posição e tamanho não contam.
		largura, altura = MinLadoLousa, MinLadoLousa
	} else {
		if n.X == nil || n.Y == nil || n.Largura == nil || n.Altura == nil {
			return 0, ErrInvalido{"faltam x, y, largura ou altura"}
		}
		x, y, largura, altura = *n.X, *n.Y, *n.Largura, *n.Altura
		if err := posicaoValida(x, y); err != nil {
			return 0, err
		}
		if err := tamanhoValido(largura, altura); err != nil {
			return 0, err
		}
	}
	if n.Z != nil {
		z = *n.Z
	}
	maximo := MaxTextoLousa
	campo := "O texto"
	if n.Tipo == "ligacao" {
		maximo, campo = MaxTituloLousa, "O rótulo"
	}
	texto, err := textoDaLousa(campo, n.Texto, maximo, n.Tipo != "ligacao")
	if err != nil {
		return 0, err
	}
	titulo, err := textoDaLousa("O título", n.Titulo, MaxTituloLousa, false)
	if err != nil {
		return 0, err
	}
	var anexo, tarefa, de, para int64
	switch n.Tipo {
	case "imagem", "video":
		if n.AnexoID <= 0 {
			return 0, ErrInvalido{"falta o anexo da " + n.Tipo}
		}
		if err := conferirAnexo(ctx, tx, n.AnexoID, l.PerfilID, n.Tipo, autor); err != nil {
			return 0, err
		}
		anexo = n.AnexoID
	case "tarefa":
		if n.TarefaRef <= 0 {
			return 0, ErrInvalido{"falta a tarefa do cartão"}
		}
		if err := conferirTarefa(ctx, tx, n.TarefaRef, l.PerfilID); err != nil {
			return 0, err
		}
		tarefa = n.TarefaRef
	case "ligacao":
		if de, err = resolver(n.De, refs); err != nil {
			return 0, err
		}
		if para, err = resolver(n.Para, refs); err != nil {
			return 0, err
		}
		if de <= 0 || para <= 0 {
			return 0, ErrInvalido{"a ligação precisa de dois itens (de e para)"}
		}
		if de == para {
			return 0, ErrInvalido{"a ligação precisa de dois itens diferentes"}
		}
		if removidos[de] || removidos[para] {
			return 0, ErrInvalido{"a ligação aponta para um item removido"}
		}
		for _, id := range []int64{de, para} {
			if err := ponta(ctx, tx, l.ID, id); err != nil {
				return 0, err
			}
		}
	}
	if n.Tipo != "imagem" && n.Tipo != "video" && n.AnexoID != 0 || n.Tipo != "tarefa" && n.TarefaRef != 0 || n.Tipo != "ligacao" && (n.De != nil || n.Para != nil) {
		return 0, ErrInvalido{"campo que não vale para este tipo de item"}
	}
	resultado, err := tx.ExecContext(ctx, `INSERT INTO lousa_elementos (lousa_id, tipo, x, y, largura, altura, z, cor, titulo, texto, anexo_id, tarefa_ref, de_id, para_id, autor, agente_id, montagem_de, versao, atualizado_em)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?)`,
		l.ID, n.Tipo, x, y, largura, altura, z, cor, titulo, texto, nulo(anexo), nulo(tarefa), nulo(de), nulo(para), autor.nome(), nulo(autor.agente), nulo(n.MontagemDe), momento)
	if err != nil {
		return 0, err
	}
	return resultado.LastInsertId()
}

func alterarElemento(ctx context.Context, tx *transacao, l Lousa, id int64, c CamposElemento, momento string) error {
	atual, err := elementosPorID(ctx, tx, []int64{id})
	if err != nil {
		return err
	}
	if len(atual) == 0 || atual[0].LousaID != l.ID {
		return ErrNaoEncontrado
	}
	e := atual[0]
	if c.X != nil {
		e.X = *c.X
	}
	if c.Y != nil {
		e.Y = *c.Y
	}
	if c.Largura != nil {
		e.Largura = *c.Largura
	}
	if c.Altura != nil {
		e.Altura = *c.Altura
	}
	if c.Z != nil {
		e.Z = *c.Z
	}
	if c.Cor != nil {
		if !umDosTipos(*c.Cor, CoresLousa) {
			return ErrInvalido{"cor desconhecida (amarelo, azul, verde, rosa, lilas ou cinza)"}
		}
		e.Cor = *c.Cor
	}
	if c.Tipo != nil && *c.Tipo != e.Tipo {
		if !umDosTipos(e.Tipo, tiposDeTexto) || !umDosTipos(*c.Tipo, tiposDeTexto) {
			return ErrInvalido{"só nota, texto e código trocam de tipo entre si"}
		}
		e.Tipo = *c.Tipo
	}
	if e.Tipo != "ligacao" {
		if err := posicaoValida(e.X, e.Y); err != nil {
			return err
		}
		if err := tamanhoValido(e.Largura, e.Altura); err != nil {
			return err
		}
	}
	if c.Texto != nil {
		maximo, campo := MaxTextoLousa, "O texto"
		if e.Tipo == "ligacao" {
			maximo, campo = MaxTituloLousa, "O rótulo"
		}
		if e.Texto, err = textoDaLousa(campo, *c.Texto, maximo, e.Tipo != "ligacao"); err != nil {
			return err
		}
	}
	if c.Titulo != nil {
		if e.Titulo, err = textoDaLousa("O título", *c.Titulo, MaxTituloLousa, false); err != nil {
			return err
		}
	}
	if c.De != nil || c.Para != nil {
		if e.Tipo != "ligacao" {
			return ErrInvalido{"só uma ligação tem pontas"}
		}
		if c.De != nil {
			e.De = *c.De
		}
		if c.Para != nil {
			e.Para = *c.Para
		}
		if e.De == e.Para {
			return ErrInvalido{"a ligação precisa de dois itens diferentes"}
		}
		for _, p := range []int64{e.De, e.Para} {
			if err := ponta(ctx, tx, l.ID, p); err != nil {
				return err
			}
		}
	}
	// Uma peça do montador que você move ou edita vira sua: remontar a
	// coluna não mexe mais nela.
	if e.Autor == "montador" {
		e.Autor = "voce"
	}
	_, err = tx.ExecContext(ctx, `UPDATE lousa_elementos SET tipo = ?, x = ?, y = ?, largura = ?, altura = ?, z = ?, cor = ?, titulo = ?, texto = ?,
		de_id = ?, para_id = ?, autor = ?, versao = versao + 1, atualizado_em = ? WHERE id = ?`,
		e.Tipo, e.X, e.Y, e.Largura, e.Altura, e.Z, e.Cor, e.Titulo, e.Texto, nulo(e.De), nulo(e.Para), e.Autor, momento, id)
	return err
}

// O agente

// NovoDoAgente: um elemento que o agente acrescenta, com um ref opcional
// para as ligações do mesmo pedido.
type NovoDoAgente struct {
	Ref string `json:"ref,omitempty"`
	NovoElemento
}

// AcrescentarDoAgente acrescenta à lousa da tarefa o que o agente mandou. O
// que vem sem x e y é posicionado numa grade à direita do que já existe, e o
// que vem sem tamanho tem o tamanho estimado pelo texto. Grava o evento
// "lousa.agente" (só as quantidades, nunca o texto).
func (b *Banco) AcrescentarDoAgente(ctx context.Context, tarefa, agente int64, novos []NovoDoAgente) (ResultadoLousa, error) {
	var r ResultadoLousa
	if len(novos) == 0 {
		return r, ErrInvalido{"nada para acrescentar"}
	}
	if len(novos) > MaxDoAgente {
		return r, ErrInvalido{fmt.Sprintf("acrescente no máximo %d itens por vez", MaxDoAgente)}
	}
	err := b.emTransacao(ctx, func(tx *transacao) error {
		var ferramenta, papel string
		err := tx.QueryRowContext(ctx, `SELECT ferramenta, papel FROM agentes WHERE id = ? AND tarefa_id = ?`, agente, tarefa).Scan(&ferramenta, &papel)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrInvalido{"o agente não é desta tarefa"}
		}
		if err != nil {
			return err
		}
		escopo, titulo, projetoNome, err := escopoDaTarefa(ctx, tx, tarefa)
		if err != nil {
			return err
		}
		id, _, err := garantirLousa(ctx, tx, DonoLousa{TarefaID: tarefa})
		if err != nil {
			return err
		}
		l, err := lerLousa(ctx, tx, id)
		if err != nil {
			return err
		}
		ocupados, err := lerElementos(ctx, tx, `WHERE e.lousa_id = ? AND e.tipo <> 'ligacao'`, l.ID)
		if err != nil {
			return err
		}
		if err := posicionarNovos(ctx, tx, ocupados, novos); err != nil {
			return err
		}
		ops := make([]Operacao, len(novos))
		tipos := map[string]int{}
		for i := range novos {
			n := novos[i].NovoElemento
			ops[i] = Operacao{Op: "criar", Ref: novos[i].Ref, Elemento: &n}
			tipos[n.Tipo]++
		}
		r, err = aplicarLote(ctx, tx, l, ops, autorLousa{agente: agente, soDaTarefa: tarefa})
		if err != nil {
			return err
		}
		ids := make([]int64, len(r.Elementos))
		for i, e := range r.Elementos {
			ids[i] = e.ID
		}
		escopo.Agente = agente
		return registrar(ctx, tx, "lousa.agente", escopo, map[string]any{
			"lousa": l.ID, "tarefa": tarefa, "titulo": titulo, "projeto_nome": projetoNome, "quantidade": len(novos), "tipos": tipos,
			"elementos": ids, "agente": agente, "ferramenta": ferramenta, "papel": papel,
		})
	})
	return r, err
}

// ApagarDoAgente apaga da lousa da tarefa os itens que o agente pediu pelo
// id, ou todos (tudo). As ligações dos itens apagados saem junto. O agente
// não acompanha versões: vale o que está gravado agora (a tela que tinha um
// desses itens aberto fica sabendo pelo aviso, como quando outra tela apaga).
// Grava o evento "lousa.agente" com a ação "apagou" (só os ids e a conta).
func (b *Banco) ApagarDoAgente(ctx context.Context, tarefa, agente int64, ids []int64, tudo bool) (ResultadoLousa, error) {
	var r ResultadoLousa
	if !tudo && len(ids) == 0 {
		return r, ErrInvalido{"diga os ids dos itens a apagar, ou tudo"}
	}
	if len(ids) > MaxOperacoesLousa {
		return r, ErrInvalido{fmt.Sprintf("apague no máximo %d itens por vez", MaxOperacoesLousa)}
	}
	err := b.emTransacao(ctx, func(tx *transacao) error {
		var ferramenta, papel string
		err := tx.QueryRowContext(ctx, `SELECT ferramenta, papel FROM agentes WHERE id = ? AND tarefa_id = ?`, agente, tarefa).Scan(&ferramenta, &papel)
		if errors.Is(err, sql.ErrNoRows) {
			return ErrInvalido{"o agente não é desta tarefa"}
		}
		if err != nil {
			return err
		}
		escopo, titulo, projetoNome, err := escopoDaTarefa(ctx, tx, tarefa)
		if err != nil {
			return err
		}
		id, _, err := garantirLousa(ctx, tx, DonoLousa{TarefaID: tarefa})
		if err != nil {
			return err
		}
		l, err := lerLousa(ctx, tx, id)
		if err != nil {
			return err
		}
		// As versões atuais: o que não está nesta lousa é recusado.
		versoes := map[int64]int64{}
		linhas, err := tx.QueryContext(ctx, `SELECT id, versao FROM lousa_elementos WHERE lousa_id = ?`, l.ID)
		if err != nil {
			return err
		}
		for linhas.Next() {
			var id, versao int64
			if err := linhas.Scan(&id, &versao); err != nil {
				linhas.Close()
				return err
			}
			versoes[id] = versao
		}
		linhas.Close()
		if tudo {
			ids = ids[:0]
			for id := range versoes {
				ids = append(ids, id)
			}
		}
		ids = slices.Compact(slices.Sorted(slices.Values(ids)))
		if len(ids) == 0 {
			r = ResultadoLousa{Lousa: l, Elementos: []Elemento{}, Removidos: []int64{}, Refs: map[string]int64{}}
			return nil
		}
		ops := make([]Operacao, len(ids))
		for i, id := range ids {
			versao, ok := versoes[id]
			if !ok {
				return ErrInvalido{fmt.Sprintf("o item %d não está na lousa desta tarefa", id)}
			}
			ops[i] = Operacao{Op: "remover", ID: id, Versao: versao}
		}
		r, err = aplicarLote(ctx, tx, l, ops, autorLousa{agente: agente, soDaTarefa: tarefa})
		if err != nil {
			return err
		}
		escopo.Agente = agente
		return registrar(ctx, tx, "lousa.agente", escopo, map[string]any{
			"lousa": l.ID, "tarefa": tarefa, "titulo": titulo, "projeto_nome": projetoNome, "acao": "apagou", "quantidade": len(r.Removidos),
			"removidos": r.Removidos, "agente": agente, "ferramenta": ferramenta, "papel": papel,
		})
	})
	return r, err
}

// Posição automática

type retangulo struct{ x, y, largura, altura float64 }

func (a retangulo) cruza(b retangulo) bool {
	return a.x < b.x+b.largura && b.x < a.x+a.largura && a.y < b.y+b.altura && b.y < a.y+a.altura
}

// Vãos da grade do agente (56: o rótulo de uma ligação cabe entre dois
// cartões ligados), a folga mínima até o que já existe e a altura de uma
// coluna antes de começar outra.
const (
	vaoGrade       = 56.0
	folgaDoAgente  = 24.0
	alturaDaColuna = 1400.0
)

// posicionarNovos preenche o tamanho e a posição do que o agente mandou sem eles.
func posicionarNovos(ctx context.Context, tx *transacao, ocupados []Elemento, novos []NovoDoAgente) error {
	var tamanhos []retangulo
	var semPosicao []int
	for i := range novos {
		n := &novos[i].NovoElemento
		if n.Tipo == "ligacao" {
			continue
		}
		if n.Largura == nil || n.Altura == nil {
			largura, altura := tamanhoEstimado(n.Tipo, n.Titulo, n.Texto)
			if n.Tipo == "imagem" && n.AnexoID > 0 {
				var l, a float64
				if tx.QueryRowContext(ctx, `SELECT largura, altura FROM anexos WHERE id = ?`, n.AnexoID).Scan(&l, &a) == nil && l > 0 && a > 0 {
					largura = math.Min(480, l)
					altura = math.Max(MinLadoLousa, math.Min(MaxLadoLousa, largura*a/l))
				}
			}
			if n.Largura == nil {
				n.Largura = &largura
			}
			if n.Altura == nil {
				n.Altura = &altura
			}
		}
		if n.X == nil || n.Y == nil {
			semPosicao = append(semPosicao, i)
			tamanhos = append(tamanhos, retangulo{largura: *n.Largura, altura: *n.Altura})
		}
	}
	existentes := make([]retangulo, 0, len(ocupados)+len(novos))
	for _, e := range ocupados {
		existentes = append(existentes, retangulo{e.X, e.Y, e.Largura, e.Altura})
	}
	// Uma posição dada que cai em cima de algo (do que já existe ou do que
	// veio antes no mesmo pedido) anda até o primeiro lugar livre.
	for i := range novos {
		n := &novos[i].NovoElemento
		if n.Tipo == "ligacao" || n.X == nil || n.Y == nil {
			continue
		}
		r := lugarLivre(retangulo{*n.X, *n.Y, *n.Largura, *n.Altura}, existentes, folgaDoAgente)
		x, y := r.x, r.y
		n.X, n.Y = &x, &y
		existentes = append(existentes, r)
	}
	for k, p := range posicaoAutomatica(existentes, tamanhos) {
		x, y := p.x, p.y
		n := &novos[semPosicao[k]].NovoElemento
		n.X, n.Y = &x, &y
	}
	return nil
}

// lugarLivre devolve r onde está, se nada cruza com ele (com a folga), ou o
// primeiro lugar livre andando para a direita ou para baixo do que está no
// caminho, o que for mais perto.
func lugarLivre(r retangulo, ocupados []retangulo, folga float64) retangulo {
	cruza := func(r retangulo) (retangulo, bool) {
		maior := retangulo{r.x - folga, r.y - folga, r.largura + 2*folga, r.altura + 2*folga}
		for _, o := range ocupados {
			if maior.cruza(o) {
				return o, true
			}
		}
		return retangulo{}, false
	}
	andar := func(paraDireita bool) (retangulo, float64) {
		atual := r
		for range len(ocupados) + 1 {
			o, ok := cruza(atual)
			if !ok {
				return atual, math.Hypot(atual.x-r.x, atual.y-r.y)
			}
			if paraDireita {
				atual.x = math.Round(o.x + o.largura + folga)
			} else {
				atual.y = math.Round(o.y + o.altura + folga)
			}
		}
		return atual, math.Inf(1)
	}
	direita, dd := andar(true)
	baixo, db := andar(false)
	if db < dd {
		return baixo
	}
	return direita
}

// posicaoAutomatica põe os tamanhos numa grade de colunas à direita de tudo
// que já existe: nada do que já está na lousa fica coberto.
func posicaoAutomatica(existentes []retangulo, tamanhos []retangulo) []retangulo {
	x0, y0 := 0.0, 0.0
	if len(existentes) > 0 {
		direita, topo := math.Inf(-1), math.Inf(1)
		for _, e := range existentes {
			direita = math.Max(direita, e.x+e.largura)
			topo = math.Min(topo, e.y)
		}
		x0, y0 = math.Round(direita+2*vaoGrade), math.Round(topo)
	}
	saida := make([]retangulo, len(tamanhos))
	x, y, coluna := x0, y0, 0.0
	for i, t := range tamanhos {
		if y > y0 && y+t.altura > y0+alturaDaColuna {
			x, y, coluna = x+coluna+vaoGrade, y0, 0
		}
		saida[i] = retangulo{x, y, t.largura, t.altura}
		y += t.altura + vaoGrade
		coluna = math.Max(coluna, t.largura)
	}
	return saida
}

// tamanhoEstimado pelo texto: código em monoespaçada (7,5 por caractere e
// 18 por linha), nota e texto pela quebra numa largura fixa.
func tamanhoEstimado(tipo, titulo, texto string) (float64, float64) {
	linhas := strings.Split(strings.TrimRight(texto, "\n"), "\n")
	faixa := 0.0
	if titulo != "" {
		faixa = 24
	}
	limitar := func(v, minimo, maximo float64) float64 { return math.Round(math.Max(minimo, math.Min(maximo, v))) }
	switch tipo {
	case "codigo":
		colunas := 0
		for _, l := range linhas {
			colunas = max(colunas, utf8.RuneCountInString(strings.ReplaceAll(l, "\t", "    ")))
		}
		return limitar(float64(colunas)*7.5+24+8, 200, 960), limitar(float64(len(linhas))*18+24+faixa, 48, MaxLadoLousa)
	case "imagem":
		return 480, 270
	case "video":
		return 320, 180
	case "tarefa":
		return 260, 96
	}
	// Nota e texto: mais largas quando têm tabela ou código (que não quebram).
	largura := 260.0
	if tipo == "texto" {
		largura = 280
	}
	for _, l := range linhas {
		if t := strings.TrimSpace(l); strings.HasPrefix(t, "|") || strings.HasPrefix(l, "    ") {
			largura = math.Max(largura, float64(utf8.RuneCountInString(l))*7.2+28)
		}
	}
	largura = limitar(largura, 160, 640)
	porLinha := math.Max(8, (largura-28)/7.2)
	altura := 0.0
	codigo := false
	for _, l := range linhas {
		t := strings.TrimSpace(l)
		switch {
		case strings.HasPrefix(t, "```"):
			codigo = !codigo
			altura += 6
		case codigo:
			altura += 17
		case strings.HasPrefix(t, "# "):
			altura += 34
		case strings.HasPrefix(t, "## "), strings.HasPrefix(t, "### "):
			altura += 28
		case strings.HasPrefix(t, "|"):
			altura += 26
		case t == "":
			altura += 10
		default:
			altura += math.Ceil(float64(max(1, utf8.RuneCountInString(t)))/porLinha) * 20
		}
	}
	return largura, limitar(altura+28+faixa+8, 48, MaxLadoLousa)
}

// O montador (a lousa da reunião)

// SubstituirMontador refaz o que é do montador numa lousa da reunião: apaga
// as peças com autor montador (as ligações delas saem junto, inclusive as
// suas que apontavam para elas) e cria as novas, com os refs resolvidos no
// lote. O que é seu, inclusive uma peça montada que você moveu ou editou,
// fica onde está. Tudo ou nada; não grava evento (decisão 0008).
func (b *Banco) SubstituirMontador(ctx context.Context, lousa int64, novos []NovoDoAgente) (ResultadoLousa, error) {
	var r ResultadoLousa
	err := b.emTransacao(ctx, func(tx *transacao) error {
		l, err := lerLousa(ctx, tx, lousa)
		if err != nil {
			return err
		}
		if !l.Dono.DaReuniao() {
			return ErrInvalido{"só a lousa da daily ou da sprint é montada pelo núcleo"}
		}
		r = ResultadoLousa{Lousa: l, Elementos: []Elemento{}, Removidos: []int64{}, Refs: map[string]int64{}}
		linhas, err := tx.QueryContext(ctx, `SELECT id FROM lousa_elementos WHERE lousa_id = ? AND (autor = 'montador'
			OR de_id IN (SELECT id FROM lousa_elementos WHERE lousa_id = ? AND autor = 'montador')
			OR para_id IN (SELECT id FROM lousa_elementos WHERE lousa_id = ? AND autor = 'montador'))`, l.ID, l.ID, l.ID)
		if err != nil {
			return err
		}
		removidos := map[int64]bool{}
		for linhas.Next() {
			var id int64
			if err := linhas.Scan(&id); err != nil {
				linhas.Close()
				return err
			}
			removidos[id] = true
		}
		linhas.Close()
		for id := range removidos {
			if _, err := tx.ExecContext(ctx, `DELETE FROM lousa_elementos WHERE id = ?`, id); err != nil {
				return err
			}
			r.Removidos = append(r.Removidos, id)
		}
		sort.Slice(r.Removidos, func(i, j int) bool { return r.Removidos[i] < r.Removidos[j] })
		var z int64
		if err := tx.QueryRowContext(ctx, `SELECT COALESCE(MAX(z), 0) FROM lousa_elementos WHERE lousa_id = ?`, l.ID).Scan(&z); err != nil {
			return err
		}
		momento := agora()
		criados := []int64{}
		for _, n := range novos {
			if n.Ref != "" {
				if _, repetido := r.Refs[n.Ref]; repetido {
					return ErrInvalido{"o mesmo ref aparece duas vezes na montagem"}
				}
			}
			z++
			id, err := criarElemento(ctx, tx, l, n.NovoElemento, r.Refs, map[int64]bool{}, z, autorLousa{montador: true}, momento)
			if err != nil {
				return err
			}
			if n.Ref != "" {
				r.Refs[n.Ref] = id
			}
			criados = append(criados, id)
		}
		var total int
		if err := tx.QueryRowContext(ctx, `SELECT COUNT(*) FROM lousa_elementos WHERE lousa_id = ?`, l.ID).Scan(&total); err != nil {
			return err
		}
		if total > MaxElementosLousa {
			return ErrInvalido{fmt.Sprintf("A lousa chegou ao limite de %d itens", MaxElementosLousa)}
		}
		r.Elementos, err = elementosPorID(ctx, tx, criados)
		return err
	})
	return r, err
}

// TarefasMontadas diz quais tarefas têm peças do montador na lousa, com a
// hora da montagem mais antiga de cada uma.
func (b *Banco) TarefasMontadas(ctx context.Context, lousa int64) (map[int64]string, error) {
	linhas, err := b.db.QueryContext(ctx, `SELECT montagem_de, MIN(atualizado_em) FROM lousa_elementos
		WHERE lousa_id = ? AND autor = 'montador' AND montagem_de IS NOT NULL GROUP BY montagem_de`, lousa)
	if err != nil {
		return nil, err
	}
	defer linhas.Close()
	tarefas := map[int64]string{}
	for linhas.Next() {
		var id int64
		var quando string
		if err := linhas.Scan(&id, &quando); err != nil {
			return nil, err
		}
		tarefas[id] = quando
	}
	return tarefas, linhas.Err()
}
