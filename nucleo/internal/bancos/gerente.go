// Package bancos conecta a Colmeia aos bancos de dados das conexões do
// perfil (PostgreSQL, MySQL/MariaDB, SQL Server e SQLite), só como cliente:
// nada aqui escuta porta. Executa uma instrução por vez, somente leitura por
// padrão, devolve as células como texto e guarda a execução aberta para o
// "carregar mais". A senha chega pronta (do chaveiro ou da memória) e nunca
// sai daqui: os erros passam por um redator.
package bancos

import (
	"context"
	"crypto/rand"
	"database/sql"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode/utf8"
)

const (
	// MaxLinhasPagina e MaxBytesPagina limitam o que vai de uma vez à tela.
	MaxLinhasPagina = 5000
	MaxBytesPagina  = 8 << 20
	// MaxCelula: cada célula vai cortada nisso.
	MaxCelula = 4 << 10
	// MaxPoolsPorConexao: bancos abertos ao mesmo tempo numa conexão.
	MaxPoolsPorConexao = 4
	// MaxConexoesPool: conexões ao servidor por banco.
	MaxConexoesPool = 3
)

// Tempos (os testes encurtam).
type Tempos struct {
	// Ocioso: um pool sem uso fecha sozinho depois disso.
	Ocioso time.Duration
	// CursorParado: a execução aberta para "carregar mais" fecha depois disso.
	CursorParado time.Duration
}

var TemposPadrao = Tempos{Ocioso: 10 * time.Minute, CursorParado: 2 * time.Minute}

// Gerente guarda os pools por (conexão, banco) e as execuções abertas.
type Gerente struct {
	Tempos Tempos

	mu        sync.Mutex
	pools     map[chavePool]*pool
	execucoes map[string]*execucao
	// cursor aberto de cada conexão (só um: abrir outro fecha o anterior).
	cursores map[int64]string
}

type chavePool struct {
	conexao int64
	banco   string
}

type pool struct {
	db     *sql.DB
	usado  time.Time
	uso    int // execuções e consultas em andamento
	timer  *time.Timer
	tipo   string
	fechou bool
}

func NovoGerente() *Gerente {
	return &Gerente{Tempos: TemposPadrao, pools: map[chavePool]*pool{}, execucoes: map[string]*execucao{}, cursores: map[int64]string{}}
}

// pegar devolve o pool (abrindo se preciso) já marcado em uso; chame o
// devolver que vem junto ao terminar.
func (g *Gerente) pegar(c Config, banco string) (*sql.DB, func(), error) {
	if c.Tipo == SQLite {
		banco = ""
	}
	chave := chavePool{c.ID, banco}
	g.mu.Lock()
	defer g.mu.Unlock()
	p := g.pools[chave]
	if p == nil {
		g.abrirEspacoSemTrava(c.ID)
		db, err := abrir(c, banco)
		if err != nil {
			return nil, nil, err
		}
		db.SetMaxOpenConns(MaxConexoesPool)
		db.SetMaxIdleConns(MaxConexoesPool)
		db.SetConnMaxIdleTime(g.Tempos.Ocioso)
		p = &pool{db: db, tipo: c.Tipo}
		g.pools[chave] = p
	}
	p.uso++
	p.usado = time.Now()
	if p.timer != nil {
		p.timer.Stop()
	}
	devolvido := false
	return p.db, func() {
		g.mu.Lock()
		defer g.mu.Unlock()
		if devolvido {
			return
		}
		devolvido = true
		p.uso--
		p.usado = time.Now()
		g.armarOciosoSemTrava(chave, p)
	}, nil
}

// armarOciosoSemTrava fecha o pool depois de Ocioso sem uso (um timer por
// pool, rearmado a cada uso; nada de varredura).
func (g *Gerente) armarOciosoSemTrava(chave chavePool, p *pool) {
	if p.uso > 0 || p.fechou {
		return
	}
	if p.timer == nil {
		p.timer = time.AfterFunc(g.Tempos.Ocioso, func() {
			g.mu.Lock()
			defer g.mu.Unlock()
			if g.pools[chave] == p && p.uso == 0 && time.Since(p.usado) >= g.Tempos.Ocioso/2 {
				p.fechou = true
				delete(g.pools, chave)
				go p.db.Close()
			}
		})
		return
	}
	p.timer.Reset(g.Tempos.Ocioso)
}

// abrirEspacoSemTrava fecha o banco menos usado da conexão quando ela já tem
// MaxPoolsPorConexao abertos (só os parados).
func (g *Gerente) abrirEspacoSemTrava(conexao int64) {
	var abertos []chavePool
	for k := range g.pools {
		if k.conexao == conexao {
			abertos = append(abertos, k)
		}
	}
	if len(abertos) < MaxPoolsPorConexao {
		return
	}
	var velho *chavePool
	for i, k := range abertos {
		p := g.pools[k]
		if p.uso > 0 {
			continue
		}
		if velho == nil || p.usado.Before(g.pools[*velho].usado) {
			velho = &abertos[i]
		}
	}
	if velho != nil {
		g.fecharPoolSemTrava(*velho)
	}
}

func (g *Gerente) fecharPoolSemTrava(k chavePool) {
	p := g.pools[k]
	if p == nil {
		return
	}
	p.fechou = true
	if p.timer != nil {
		p.timer.Stop()
	}
	delete(g.pools, k)
	go p.db.Close()
}

// FecharConexao fecha os pools e as execuções da conexão (editada,
// removida, desconectada ou com a senha trocada).
func (g *Gerente) FecharConexao(conexao int64) {
	g.mu.Lock()
	var execucoes []*execucao
	for _, e := range g.execucoes {
		if e.conexao == conexao {
			execucoes = append(execucoes, e)
		}
	}
	for k := range g.pools {
		if k.conexao == conexao {
			g.fecharPoolSemTrava(k)
		}
	}
	g.mu.Unlock()
	for _, e := range execucoes {
		e.cancelarCom(ErrFechada)
		e.fechar()
	}
}

// FecharTodas encerra tudo (o núcleo vai desligar).
func (g *Gerente) FecharTodas() {
	g.mu.Lock()
	var conexoes []int64
	vistas := map[int64]bool{}
	for k := range g.pools {
		if !vistas[k.conexao] {
			vistas[k.conexao] = true
			conexoes = append(conexoes, k.conexao)
		}
	}
	for _, e := range g.execucoes {
		if !vistas[e.conexao] {
			vistas[e.conexao] = true
			conexoes = append(conexoes, e.conexao)
		}
	}
	g.mu.Unlock()
	for _, c := range conexoes {
		g.FecharConexao(c)
	}
}

// Abertos diz quantos pools e execuções há (para os testes).
func (g *Gerente) Abertos() (pools, execucoes int) {
	g.mu.Lock()
	defer g.mu.Unlock()
	return len(g.pools), len(g.execucoes)
}

// Teste da conexão

// InfoTeste é o resultado do "Testar".
type InfoTeste struct {
	Ms       int64  `json:"ms"`
	Servidor string `json:"servidor"`
	TLS      bool   `json:"tls"`
}

// Testar abre uma conexão avulsa (fora dos pools), confere e fecha.
func Testar(ctx context.Context, c Config) (InfoTeste, error) {
	inicio := time.Now()
	db, err := abrir(c, "")
	if err != nil {
		return InfoTeste{}, err
	}
	defer db.Close()
	ctx, cancelar := context.WithTimeout(ctx, TempoParaConectar+5*time.Second)
	defer cancelar()
	conn, err := db.Conn(ctx)
	if err != nil {
		return InfoTeste{}, err
	}
	defer conn.Close()
	if err := conn.PingContext(ctx); err != nil {
		return InfoTeste{}, err
	}
	info := InfoTeste{Ms: time.Since(inicio).Milliseconds()}
	info.Servidor = versaoDoServidor(ctx, conn, c.Tipo)
	info.TLS = usouTLS(ctx, conn, c.Tipo)
	return info, nil
}

// Árvore

// Bancos lista os bancos do servidor (no MySQL, os esquemas).
func (g *Gerente) Bancos(ctx context.Context, c Config) ([]string, error) {
	db, devolver, err := g.pegar(c, "")
	if err != nil {
		return nil, err
	}
	defer devolver()
	nomes, err := listarBancos(ctx, db, c.Tipo)
	return nomes, erroDoBanco(err, c.Senha, "")
}

// Esquemas lista os esquemas de um banco (Postgres e SQL Server).
func (g *Gerente) Esquemas(ctx context.Context, c Config, banco string) ([]string, error) {
	db, devolver, err := g.pegar(c, bancoDoCatalogo(c, banco))
	if err != nil {
		return nil, err
	}
	defer devolver()
	nomes, err := listarEsquemas(ctx, db, c.Tipo, banco)
	return nomes, erroDoBanco(err, c.Senha, "")
}

// Objetos lista as tabelas e views do esquema (no MySQL, do banco).
func (g *Gerente) Objetos(ctx context.Context, c Config, banco, esquema string) ([]Objeto, int, error) {
	db, devolver, err := g.pegar(c, bancoDoCatalogo(c, banco))
	if err != nil {
		return nil, 0, err
	}
	defer devolver()
	objetos, total, err := listarObjetos(ctx, db, c.Tipo, banco, esquema)
	return objetos, total, erroDoBanco(err, c.Senha, "")
}

// Colunas de uma tabela ou view.
func (g *Gerente) Colunas(ctx context.Context, c Config, banco, esquema, objeto string) ([]ColunaTabela, error) {
	db, devolver, err := g.pegar(c, bancoDoCatalogo(c, banco))
	if err != nil {
		return nil, err
	}
	defer devolver()
	colunas, err := listarColunas(ctx, db, c.Tipo, banco, esquema, objeto)
	return colunas, erroDoBanco(err, c.Senha, "")
}

// bancoDoCatalogo: no Postgres o catálogo é do banco conectado; no MySQL e
// no SQL Server, qualquer conexão enxerga os outros bancos pelo nome.
func bancoDoCatalogo(c Config, banco string) string {
	if c.Tipo == Postgres {
		return banco
	}
	return ""
}

// Previa roda o SELECT das primeiras linhas de uma tabela que o catálogo confirma.
func (g *Gerente) Previa(ctx context.Context, c Config, banco, esquema, objeto string, limite int) (Resultado, string, error) {
	db, devolver, err := g.pegar(c, bancoDoCatalogo(c, banco))
	if err != nil {
		return Resultado{}, "", err
	}
	existe, err := existeObjeto(ctx, db, c.Tipo, banco, esquema, objeto)
	devolver()
	if err != nil {
		return Resultado{}, "", erroDoBanco(err, c.Senha, "")
	}
	if !existe {
		return Resultado{}, "", ErrBanco{Mensagem: "a tabela não existe mais: " + objeto}
	}
	sql := SQLPrevia(c.Tipo, banco, esquema, objeto, limite)
	r, err := g.Executar(ctx, c, Opcoes{SQL: sql, Banco: banco, Limite: limite, Tempo: 30 * time.Second, SemCursor: true})
	return r, sql, err
}

// Execução

// Opcoes de uma execução.
type Opcoes struct {
	// Ficha é o nome da execução, escolhido pela tela (para cancelar e
	// carregar mais). Vazia, a execução não fica aberta.
	Ficha  string
	SQL    string
	Banco  string
	Limite int
	Tempo  time.Duration
	// Confirmada: você confirmou esta alteração.
	Confirmada bool
	// SemCursor: devolve a primeira página e fecha (prévia, agente).
	SemCursor bool
}

// ColunaResultado é uma coluna da grade.
type ColunaResultado struct {
	Nome   string `json:"nome"`
	Tipo   string `json:"tipo"`
	Numero bool   `json:"numero,omitempty"`
}

// Resultado de uma execução (ou de uma página a mais).
type Resultado struct {
	Colunas []ColunaResultado `json:"colunas"`
	// Linhas: cada célula é texto ou null.
	Linhas [][]*string `json:"linhas"`
	// Mais: há mais linhas para carregar.
	Mais     bool   `json:"mais"`
	Ms       int64  `json:"ms"`
	Afetadas *int64 `json:"afetadas,omitempty"`
	Verbo    string `json:"verbo"`
	Altera   bool   `json:"altera,omitempty"`
}

// execucao é uma consulta em andamento ou aberta para "carregar mais".
type execucao struct {
	g       *Gerente
	ficha   string
	conexao int64
	tipo    string
	senha   string
	sql     string
	limite  int
	tempo   time.Duration

	cancelar context.CancelFunc
	ctx      context.Context
	devolver func()
	db       *sql.DB
	conn     *sql.Conn
	tx       *sql.Tx
	linhas   *sql.Rows
	colunas  []ColunaResultado
	pendente []*string
	idMySQL  int64

	mu       sync.Mutex // a página sendo lida
	muMotivo sync.Mutex
	motivo   error // por que foi cancelada
	parada   *time.Timer
	fechada  bool
}

func (e *execucao) cancelarCom(motivo error) {
	e.muMotivo.Lock()
	if e.motivo == nil {
		e.motivo = motivo
	}
	e.muMotivo.Unlock()
	e.cancelar()
	// No MySQL, cancelar o contexto só fecha a conexão daqui: o servidor
	// continua executando. KILL QUERY por outra conexão do mesmo pool.
	if e.tipo == MySQL && e.idMySQL > 0 && e.db != nil {
		go func() {
			ctx, cancelar := context.WithTimeout(context.Background(), 5*time.Second)
			defer cancelar()
			e.db.ExecContext(ctx, "KILL QUERY "+strconv.FormatInt(e.idMySQL, 10))
		}()
	}
}

func (e *execucao) motivoDoCancelamento() error {
	e.muMotivo.Lock()
	defer e.muMotivo.Unlock()
	return e.motivo
}

// fase roda uma etapa (a primeira página ou mais uma) com o tempo-limite e
// ligada ao pedido de quem espera: se a tela desistir, a consulta para.
func (e *execucao) fase(pedido context.Context, f func() error) error {
	tempo := time.AfterFunc(e.tempo, func() { e.cancelarCom(ErrTempoEsgotado) })
	parar := context.AfterFunc(pedido, func() { e.cancelarCom(ErrCancelada) })
	err := f()
	tempo.Stop()
	parar()
	if err != nil {
		if motivo := e.motivoDoCancelamento(); motivo != nil {
			return motivo
		}
	}
	return err
}

// fechar solta tudo da execução; pode ser chamado mais de uma vez.
func (e *execucao) fechar() {
	e.g.mu.Lock()
	if e.g.execucoes[e.ficha] == e {
		delete(e.g.execucoes, e.ficha)
	}
	if e.g.cursores[e.conexao] == e.ficha {
		delete(e.g.cursores, e.conexao)
	}
	if e.parada != nil {
		e.parada.Stop()
	}
	e.g.mu.Unlock()
	e.mu.Lock()
	defer e.mu.Unlock()
	if e.fechada {
		return
	}
	e.fechada = true
	if e.linhas != nil {
		e.linhas.Close()
	}
	if e.tx != nil {
		e.tx.Rollback()
	}
	if e.conn != nil {
		e.conn.Close()
	}
	e.cancelar()
	if e.devolver != nil {
		e.devolver()
	}
}

// FichaValida: 16 a 64 letras, algarismos, - ou _.
func FichaValida(f string) bool {
	if len(f) < 16 || len(f) > 64 {
		return false
	}
	for _, c := range f {
		if !(c == '-' || c == '_' || (c >= '0' && c <= '9') || (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z')) {
			return false
		}
	}
	return true
}

func fichaAleatoria() string {
	var b [16]byte
	rand.Read(b[:])
	return "interna-" + hex.EncodeToString(b[:])
}

// Executar roda uma instrução. Leitura vai numa transação só de leitura,
// sempre desfeita no fim; alteração exige escrita ligada na conexão e a sua
// confirmação, e roda como instrução avulsa (gravada ao terminar).
func (g *Gerente) Executar(pedido context.Context, c Config, op Opcoes) (Resultado, error) {
	cl, err := Classificar(c.Tipo, op.SQL)
	if err != nil {
		return Resultado{}, err
	}
	if cl.Classe == Altera && !c.Escrita {
		return Resultado{}, ErrSomenteLeitura
	}
	if cl.Classe == Altera && !op.Confirmada {
		return Resultado{}, ErrPrecisaConfirmar{cl}
	}
	if op.Limite <= 0 || op.Limite > MaxLinhasPagina {
		op.Limite = 500
	}
	if op.Tempo <= 0 {
		op.Tempo = 30 * time.Second
	}
	if op.Ficha == "" {
		op.Ficha, op.SemCursor = fichaAleatoria(), true
	}
	ctx, cancelar := context.WithCancel(context.Background())
	e := &execucao{g: g, ficha: op.Ficha, conexao: c.ID, tipo: c.Tipo, senha: c.Senha, sql: op.SQL, limite: op.Limite, tempo: op.Tempo,
		cancelar: cancelar, ctx: ctx}
	g.mu.Lock()
	if _, existe := g.execucoes[op.Ficha]; existe {
		g.mu.Unlock()
		cancelar()
		return Resultado{}, ErrFichaEmUso
	}
	g.execucoes[op.Ficha] = e
	g.mu.Unlock()
	r, err := g.executar(pedido, c, op, cl, e)
	if err != nil || !r.Mais || op.SemCursor {
		e.fechar()
		return r, erroDoBanco(err, c.Senha, op.SQL)
	}
	// Fica aberta para "carregar mais": uma por conexão, fechada depois de parada.
	g.mu.Lock()
	anterior := g.cursores[c.ID]
	g.cursores[c.ID] = op.Ficha
	e.parada = time.AfterFunc(g.Tempos.CursorParado, e.fechar)
	velha := g.execucoes[anterior]
	g.mu.Unlock()
	if velha != nil && anterior != op.Ficha {
		velha.fechar()
	}
	return r, nil
}

func (g *Gerente) executar(pedido context.Context, c Config, op Opcoes, cl Classificacao, e *execucao) (Resultado, error) {
	r := Resultado{Verbo: cl.Verbo, Altera: cl.Classe == Altera}
	db, devolver, err := g.pegar(c, op.Banco)
	if err != nil {
		return r, err
	}
	e.devolver, e.db = devolver, db
	inicio := time.Now()
	err = e.fase(pedido, func() error {
		e.mu.Lock()
		defer e.mu.Unlock()
		if e.fechada {
			return ErrCancelada
		}
		conn, err := db.Conn(e.ctx)
		if err != nil {
			return err
		}
		e.conn = conn
		if c.Tipo == MySQL {
			if err := conn.QueryRowContext(e.ctx, "SELECT CONNECTION_ID()").Scan(&e.idMySQL); err != nil {
				return err
			}
			if _, err := conn.ExecContext(e.ctx, "SET SESSION max_execution_time = "+strconv.FormatInt(op.Tempo.Milliseconds(), 10)); err != nil {
				return err
			}
		}
		if cl.Classe == Altera {
			// Alteração confirmada: direto, como uma instrução avulsa (que o
			// servidor já grava inteira ou nada). Fora de transação, porque
			// CREATE DATABASE, VACUUM e afins não rodam dentro de uma.
			res, err := conn.ExecContext(e.ctx, op.SQL)
			if err != nil {
				return err
			}
			n, _ := res.RowsAffected()
			r.Afetadas = &n
			return nil
		}
		opcoesTx := &sql.TxOptions{ReadOnly: c.Tipo == Postgres || c.Tipo == MySQL}
		tx, err := conn.BeginTx(e.ctx, opcoesTx)
		if err != nil {
			return err
		}
		e.tx = tx
		if c.Tipo == Postgres {
			if _, err := tx.ExecContext(e.ctx, "SET LOCAL statement_timeout = "+strconv.FormatInt(op.Tempo.Milliseconds(), 10)); err != nil {
				return err
			}
		}
		linhas, err := tx.QueryContext(e.ctx, op.SQL)
		if err != nil {
			return err
		}
		e.linhas = linhas
		tipos, err := linhas.ColumnTypes()
		if err != nil {
			return err
		}
		e.colunas = make([]ColunaResultado, len(tipos))
		for i, t := range tipos {
			e.colunas[i] = ColunaResultado{Nome: t.Name(), Tipo: strings.ToLower(t.DatabaseTypeName()), Numero: tipoNumerico(t.DatabaseTypeName())}
		}
		return e.lerPagina(&r, op.Limite)
	})
	r.Ms = time.Since(inicio).Milliseconds()
	r.Colunas = e.colunas
	if r.Colunas == nil {
		r.Colunas = []ColunaResultado{}
	}
	if r.Linhas == nil {
		r.Linhas = [][]*string{}
	}
	marcarNumeros(r.Colunas, r.Linhas)
	return r, err
}

// lerPagina lê até `limite` linhas (e até MaxBytesPagina) da execução. Para
// saber se há mais, lê uma linha além, que fica guardada para a próxima página.
func (e *execucao) lerPagina(r *Resultado, limite int) error {
	n := len(e.colunas)
	valores := make([]any, n)
	ponteiros := make([]any, n)
	for i := range valores {
		ponteiros[i] = &valores[i]
	}
	ler := func() ([]*string, bool, error) {
		if !e.linhas.Next() {
			return nil, false, e.linhas.Err()
		}
		if err := e.linhas.Scan(ponteiros...); err != nil {
			return nil, false, err
		}
		linha := make([]*string, n)
		for i, v := range valores {
			linha[i] = Celula(v, e.colunas[i].Tipo)
			valores[i] = nil
		}
		return linha, true, nil
	}
	bytes := 0
	for len(r.Linhas) < limite && bytes < MaxBytesPagina {
		linha := e.pendente
		e.pendente = nil
		if linha == nil {
			var ok bool
			var err error
			if linha, ok, err = ler(); err != nil || !ok {
				r.Mais = false
				return err
			}
		}
		for _, c := range linha {
			if c != nil {
				bytes += len(*c)
			}
		}
		r.Linhas = append(r.Linhas, linha)
	}
	if e.pendente == nil {
		linha, ok, err := ler()
		if err != nil {
			return err
		}
		e.pendente = linha
		r.Mais = ok
		return nil
	}
	r.Mais = true
	return nil
}

// Mais lê a próxima página de uma execução aberta.
func (g *Gerente) Mais(pedido context.Context, ficha string, limite int) (Resultado, error) {
	g.mu.Lock()
	e := g.execucoes[ficha]
	if e != nil && e.parada != nil {
		e.parada.Stop()
	}
	g.mu.Unlock()
	if e == nil || e.linhas == nil {
		return Resultado{}, ErrSemExecucao
	}
	if limite <= 0 || limite > MaxLinhasPagina {
		limite = e.limite
	}
	inicio := time.Now()
	r := Resultado{Verbo: "SELECT", Colunas: e.colunas, Linhas: [][]*string{}}
	err := e.fase(pedido, func() error {
		e.mu.Lock()
		defer e.mu.Unlock()
		if e.fechada {
			return ErrSemExecucao
		}
		return e.lerPagina(&r, limite)
	})
	r.Ms = time.Since(inicio).Milliseconds()
	if err != nil || !r.Mais {
		e.fechar()
		return r, erroDoBanco(err, e.senha, e.sql)
	}
	g.mu.Lock()
	if e.parada != nil {
		e.parada.Reset(g.Tempos.CursorParado)
	}
	g.mu.Unlock()
	return r, nil
}

// Cancelar para a execução em andamento (ou fecha a aberta). Diz se havia.
func (g *Gerente) Cancelar(ficha string) bool {
	g.mu.Lock()
	e := g.execucoes[ficha]
	g.mu.Unlock()
	if e == nil {
		return false
	}
	e.cancelarCom(ErrCancelada)
	go e.fechar()
	return true
}

// Células

var tiposNumericos = []string{"INT", "SERIAL", "DECIMAL", "NUMERIC", "FLOAT", "DOUBLE", "REAL", "MONEY", "NUMBER"}

func tipoNumerico(tipo string) bool {
	t := strings.ToUpper(tipo)
	if strings.Contains(t, "INTERVAL") || strings.Contains(t, "POINT") {
		return false
	}
	for _, n := range tiposNumericos {
		if strings.Contains(t, n) {
			return true
		}
	}
	return false
}

// marcarNumeros: sem tipo declarado (SQLite), a coluna é numérica se todos
// os valores da página são números.
func marcarNumeros(colunas []ColunaResultado, linhas [][]*string) {
	for i := range colunas {
		if colunas[i].Tipo != "" || len(linhas) == 0 {
			continue
		}
		todos, algum := true, false
		for _, l := range linhas {
			if l[i] == nil {
				continue
			}
			algum = true
			if _, err := strconv.ParseFloat(*l[i], 64); err != nil {
				todos = false
				break
			}
		}
		colunas[i].Numero = todos && algum
	}
}

var tiposBinarios = []string{"BYTEA", "BLOB", "BINARY", "IMAGE", "VARBINARY"}

func tipoBinario(tipo string) bool {
	t := strings.ToUpper(tipo)
	for _, b := range tiposBinarios {
		if strings.Contains(t, b) {
			return true
		}
	}
	return false
}

func tamanhoLegivel(n int) string {
	if n == 1 {
		return "1 byte"
	}
	if n < 1024 {
		return fmt.Sprintf("%d bytes", n)
	}
	return fmt.Sprintf("%d KB", (n+1023)/1024)
}

// Celula transforma o valor do driver em texto (nil é NULL), cortado em
// MaxCelula. Binário vira "<binário 12 KB>".
func Celula(v any, tipo string) *string {
	var s string
	switch x := v.(type) {
	case nil:
		return nil
	case []byte:
		if tipoBinario(tipo) || !utf8.Valid(x) || strings.IndexByte(string(x), 0) >= 0 || (!tipoTexto(tipo) && temControle(x)) {
			s = "<binário " + tamanhoLegivel(len(x)) + ">"
		} else {
			s = string(x)
		}
	case string:
		s = x
	case time.Time:
		formato := "2006-01-02 15:04:05.999999"
		if x.Hour() == 0 && x.Minute() == 0 && x.Second() == 0 && x.Nanosecond() == 0 && (x.Location() == time.UTC || tipoSoData(tipo)) {
			formato = "2006-01-02"
		}
		if x.Location() != time.UTC {
			formato += " -07:00"
		}
		s = x.Format(formato)
	case bool:
		s = strconv.FormatBool(x)
	case int64:
		s = strconv.FormatInt(x, 10)
	case float64:
		s = strconv.FormatFloat(x, 'g', -1, 64)
	case [16]byte:
		s = fmt.Sprintf("%x-%x-%x-%x-%x", x[0:4], x[4:6], x[6:8], x[8:10], x[10:16])
	case fmt.Stringer:
		s = x.String()
	default:
		if bruto, err := json.Marshal(x); err == nil {
			s = string(bruto)
		} else {
			s = fmt.Sprint(x)
		}
	}
	if len(s) > MaxCelula {
		corte := MaxCelula
		for corte > 0 && !utf8.RuneStart(s[corte]) {
			corte--
		}
		s = s[:corte] + "…"
	}
	return &s
}

func tipoSoData(tipo string) bool { return strings.EqualFold(tipo, "date") }

// tipoTexto: colunas declaradas como texto (nelas, caracteres de controle
// são texto mesmo; a tela os mostra como símbolos).
func tipoTexto(tipo string) bool {
	t := strings.ToUpper(tipo)
	for _, palavra := range []string{"CHAR", "TEXT", "CLOB", "STRING", "JSON", "XML"} {
		if strings.Contains(t, palavra) {
			return true
		}
	}
	return false
}

// temControle: bytes de controle fora tabulação e quebras de linha (x'0102'
// no SQLite chega como UTF-8 válido, mas é binário).
func temControle(b []byte) bool {
	for _, c := range b {
		if (c < ' ' && c != '\t' && c != '\n' && c != '\r') || c == 0x7f {
			return true
		}
	}
	return false
}

// Erros que a API trata pelo tipo.
func EhDoBanco(err error) bool {
	var b ErrBanco
	return errors.As(err, &b)
}
