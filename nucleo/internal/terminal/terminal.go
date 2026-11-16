// Package terminal mantém os terminais dos agentes. O núcleo é dono deles:
// fechar a tela não encerra o terminal, e quem se conecta recebe o histórico.
package terminal

import (
	"bytes"
	"io"
	"log"
	"slices"
	"strings"
	"sync"
	"sync/atomic"
	"time"
)

const (
	tamanhoHistorico = 256 * 1024 // buffer fixo por terminal
	tamanhoReenvio   = 128 * 1024 // ao conectar, só o que cabe na rolagem da tela
	// Acima disso sem confirmação da tela, o núcleo para de ler o terminal:
	// o programa que escreve fica esperando e o processador descansa.
	LimiteSemConfirmacao = 1024 * 1024

	IntervaloPadrao = 16 * time.Millisecond
	intervaloMinimo = 8 * time.Millisecond
	intervaloMaximo = 5 * time.Second

	MaxColunas = 500
	MaxLinhas  = 300
)

// Pty é o lado do núcleo de um pseudo-terminal.
type Pty interface {
	io.ReadWriteCloser
	Redimensionar(colunas, linhas uint16) error
	// Esperar bloqueia até o programa terminar e diz como ele saiu.
	Esperar() Saida
}

// Saida é como o programa do terminal terminou. Morto por um sinal, o código
// segue a convenção do shell: 128 + o número do sinal.
type Saida struct {
	Codigo   int  `json:"codigo"`
	PorSinal bool `json:"por_sinal"`
}

// Quem encerrou o terminal, quando foi a Colmeia (e não o próprio programa).
const (
	PelaRemocao   = "removido"
	PeloDesligar  = "nucleo_encerrado"
	esperaLeitura = 5 * time.Second
)

// Mudanca é o que o núcleo precisa saber de um agente: que iniciou, que mudou
// de estado (trabalhando, ocioso, aguardando) ou que terminou.
type Mudanca struct {
	Tipo   string // "iniciou", "estado" ou "terminou"
	Estado string
	// Motivo vem sempre de uma lista fixa (veja detector.go), nunca do terminal.
	Motivo string
	Desde  time.Time
	// Só em "terminou".
	Saida       Saida
	PelaColmeia string
	Duracao     time.Duration
	Trabalhando time.Duration
	Aguardando  time.Duration
}

// Cliente é uma tela conectada a um terminal.
type Cliente struct {
	pendente       []byte        // saída ainda não enviada; protegido por Sessao.mu
	Aviso          chan struct{} // acorda o envio quando chega saída nova
	intervalo      atomic.Int64  // tempo mínimo entre envios, em nanossegundos
	semConfirmacao atomic.Int64  // bytes enviados que a tela ainda não desenhou
}

func (c *Cliente) Intervalo() time.Duration { return time.Duration(c.intervalo.Load()) }

// DefinirIntervalo muda o ritmo de envio, sempre dentro dos limites.
func (c *Cliente) DefinirIntervalo(d time.Duration) {
	c.intervalo.Store(int64(min(max(d, intervaloMinimo), intervaloMaximo)))
	c.acordar()
}

func (c *Cliente) acordar() {
	select {
	case c.Aviso <- struct{}{}:
	default:
	}
}

type Sessao struct {
	ID        int64
	pty       Pty
	fim       chan struct{} // fechado quando o programa do terminal termina
	lido      chan struct{} // fechado quando Ler termina, depois de avisar o fim
	lendo     atomic.Bool
	mu        sync.Mutex
	liberado  *sync.Cond
	historico []byte
	tela      *tela // a grade do terminal, para ler o texto como está na tela
	// escritos conta os bytes que já passaram pelo histórico; corte é a
	// contagem na última escrita da tela (ou no eco dela). O detector olha só
	// o que veio depois do corte: um pedido já respondido e o eco do que você
	// digitou ficam para trás.
	escritos int64
	corte    int64
	clientes map[*Cliente]struct{}
	bytes    *atomic.Int64
	// colagem: o programa ligou o modo de colagem (ESC[?2004h) e ainda não
	// desligou; cauda é o fim da última leitura, para achar a sequência
	// partida entre duas leituras.
	colagem atomic.Bool
	cauda   []byte

	// Acompanhamento do agente (nil nos terminais de teste).
	atividade    *atividade
	aoMudar      func(Mudanca)
	encerradaPor atomic.Value // string: quem da Colmeia fechou

	// Fim de um terminal sem acompanhamento (as execuções do projeto).
	aoTerminar func(Saida, string)
}

func NovaSessao(id int64, pty Pty, bytes *atomic.Int64) *Sessao {
	s := &Sessao{ID: id, pty: pty, fim: make(chan struct{}), lido: make(chan struct{}), clientes: map[*Cliente]struct{}{}, bytes: bytes}
	s.liberado = sync.NewCond(&s.mu)
	s.tela = novaTela(TamanhoPadrao)
	return s
}

// DefinirTamanho conta à grade o tamanho com que o terminal foi aberto.
func (s *Sessao) DefinirTamanho(t Tamanho) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.tela.redimensionar(t)
}

// Acompanhar liga o acompanhamento de atividade: a sessão passa a avisar
// quando o agente trabalha, para ou parece esperar você. Chamar antes de Ler.
func (s *Sessao) Acompanhar(ferramenta string, tempos Tempos) {
	s.atividade = novaAtividade(ferramenta, tempos, s.ultimos, s.avisar)
}

// AoTerminar avisa como o programa saiu (e se foi a Colmeia que fechou),
// num terminal sem acompanhamento. Chamar antes de Ler.
func (s *Sessao) AoTerminar(f func(saida Saida, pelaColmeia string)) {
	s.aoTerminar = f
}

func (s *Sessao) avisar(m Mudanca) {
	if s.aoMudar != nil {
		s.aoMudar(m)
	}
}

// Segurar deixa o agente em "aguardando" com o motivo dado (um pedido dele
// espera a sua aprovação na tela); Soltar devolve. Num terminal sem
// acompanhamento, não fazem nada.
func (s *Sessao) Segurar(motivo string) {
	if s.atividade != nil {
		s.atividade.segurar(motivo)
	}
}

func (s *Sessao) Soltar() {
	if s.atividade != nil {
		s.atividade.soltar()
	}
}

// Estado diz o que o agente está fazendo agora e desde quando.
func (s *Sessao) Estado() (estado, motivo string, desde time.Time) {
	if s.atividade == nil {
		return "", "", time.Time{}
	}
	return s.atividade.atual()
}

// ultimos devolve o fim do histórico depois do corte, para o detector olhar
// só o que o programa escreveu desde a sua última escrita.
func (s *Sessao) ultimos(n int) []byte {
	s.mu.Lock()
	defer s.mu.Unlock()
	depois := int(min(s.escritos-s.corte, int64(len(s.historico))))
	inicio := len(s.historico) - min(n, depois)
	return append([]byte(nil), s.historico[inicio:]...)
}

// Recente devolve o fim do histórico (até n bytes), tudo o que está na tela,
// sem o corte do detector: é o que a abelha fixa mostra como texto.
func (s *Sessao) Recente(n int) []byte {
	s.mu.Lock()
	defer s.mu.Unlock()
	inicio := len(s.historico) - min(n, len(s.historico))
	return append([]byte(nil), s.historico[inicio:]...)
}

// LinhasRecentes devolve as últimas `linhas` linhas do terminal como texto
// puro, como estão na tela: sem cor, com o cursor e as reescritas já
// resolvidos pela grade. Serve para ler fora da Colmeia, onde não há emulador.
func (s *Sessao) LinhasRecentes(linhas int) []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.tela.texto(linhas)
}

// congestionado diz se alguma tela está atrasada demais. Chamar com mu travado.
func (s *Sessao) congestionado() bool {
	for c := range s.clientes {
		if len(c.pendente) > LimiteSemConfirmacao || c.semConfirmacao.Load() > LimiteSemConfirmacao {
			return true
		}
	}
	return false
}

// Ler repassa a saída do terminal para as telas até o terminal fechar.
func (s *Sessao) Ler() {
	s.lendo.Store(true)
	defer close(s.lido)
	buf := make([]byte, 32*1024)
	for {
		s.mu.Lock()
		for s.congestionado() {
			s.liberado.Wait()
		}
		s.mu.Unlock()

		n, err := s.pty.Read(buf)
		if n > 0 {
			s.bytes.Add(int64(n))
			s.guardar(buf[:n])
			if s.atividade != nil {
				s.atividade.saida(buf[:n])
			}
		}
		if err != nil {
			// O conteúdo do terminal nunca vai para o log.
			log.Printf("terminal %d encerrado", s.ID)
			close(s.fim)
			s.terminou()
			return
		}
	}
}

// guardar leva a saída às telas e ao histórico.
func (s *Sessao) guardar(b []byte) {
	s.mu.Lock()
	defer s.mu.Unlock()
	for c := range s.clientes {
		c.pendente = append(c.pendente, b...)
		c.acordar()
	}
	s.historico = append(s.historico, b...)
	s.escritos += int64(len(b))
	s.tela.alimentar(b)
	s.olharColagem(b)
	// O eco da digitação e o redesenho logo depois de uma escrita também
	// ficam antes do corte (a tela das ferramentas redesenha a caixa de texto
	// inteira a cada tecla).
	if s.atividade != nil && s.atividade.emEco() {
		s.corte = s.escritos
	}
	// Corta só ao passar do dobro, para não copiar o histórico a cada leitura.
	if len(s.historico) > 2*tamanhoHistorico {
		s.historico = append(make([]byte, 0, 2*tamanhoHistorico), s.historico[len(s.historico)-tamanhoHistorico:]...)
	}
}

var (
	colagemLigada    = []byte("\x1b[?2004h")
	colagemDesligada = []byte("\x1b[?2004l")
)

// olharColagem acompanha o modo de colagem pela saída do programa (a última
// sequência vale). Chamada com s.mu travado.
func (s *Sessao) olharColagem(b []byte) {
	olhar := func(trecho []byte) {
		liga, desliga := bytes.LastIndex(trecho, colagemLigada), bytes.LastIndex(trecho, colagemDesligada)
		if liga > desliga {
			s.colagem.Store(true)
		} else if desliga > liga {
			s.colagem.Store(false)
		}
	}
	borda := len(colagemLigada) - 1
	if len(s.cauda) > 0 {
		olhar(append(append([]byte(nil), s.cauda...), b[:min(len(b), borda)]...))
	}
	olhar(b)
	junto := append(s.cauda, b[max(0, len(b)-borda):]...)
	s.cauda = append(s.cauda[:0], junto[max(0, len(junto)-borda):]...)
}

// ColagemLigada diz se o programa do terminal pediu o modo de colagem.
func (s *Sessao) ColagemLigada() bool { return s.colagem.Load() }

// marcarEntrada registra uma escrita da tela: o eco que volta não é trabalho
// e o que estava antes não conta mais para o detector.
func (s *Sessao) marcarEntrada() {
	if s.atividade == nil {
		return
	}
	s.atividade.entrada()
	s.mu.Lock()
	s.corte = s.escritos
	s.mu.Unlock()
}

// terminou espera o processo sair e avisa como foi.
func (s *Sessao) terminou() {
	if s.atividade == nil {
		if s.aoTerminar != nil {
			saida := s.pty.Esperar()
			por, _ := s.encerradaPor.Load().(string)
			s.aoTerminar(saida, por)
		}
		return
	}
	saida := s.pty.Esperar()
	m := s.atividade.parar()
	m.Tipo = "terminou"
	m.Saida = saida
	m.PelaColmeia, _ = s.encerradaPor.Load().(string)
	s.avisar(m)
}

// Conectar registra uma tela e devolve o fim do histórico para ela desenhar.
func (s *Sessao) Conectar(intervalo time.Duration) (*Cliente, []byte) {
	s.mu.Lock()
	defer s.mu.Unlock()
	c := &Cliente{Aviso: make(chan struct{}, 1)}
	c.intervalo.Store(int64(min(max(intervalo, intervaloMinimo), intervaloMaximo)))
	inicio := max(0, len(s.historico)-tamanhoReenvio)
	historico := append([]byte(nil), s.historico[inicio:]...)
	c.semConfirmacao.Store(int64(len(historico)))
	s.clientes[c] = struct{}{}
	return c, historico
}

// Retirar pega o que está pendente para a tela e libera a leitura se ela esperava.
func (s *Sessao) Retirar(c *Cliente) []byte {
	s.mu.Lock()
	defer s.mu.Unlock()
	bloco := c.pendente
	c.pendente = nil
	c.semConfirmacao.Add(int64(len(bloco)))
	s.liberado.Broadcast()
	return bloco
}

// Confirmar registra que a tela desenhou `n` bytes.
func (s *Sessao) Confirmar(c *Cliente, n int64) {
	if n <= 0 {
		return
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	c.semConfirmacao.Add(-n)
	s.liberado.Broadcast()
}

func (s *Sessao) Desconectar(c *Cliente) {
	s.mu.Lock()
	defer s.mu.Unlock()
	delete(s.clientes, c)
	s.liberado.Broadcast()
}

// Escrever manda a digitação ao programa. O eco que volta logo depois não
// conta como trabalho do agente. O Ctrl+Z (0x1a) nunca chega: o Claude
// Code e os outros se suspendem com ele, e aqui não há shell para trazer
// o programa de volta; o terminal ficaria mudo até o agente ser removido.
func (s *Sessao) Escrever(dados []byte) (int, error) {
	s.marcarEntrada()
	dados = SemSuspender(dados)
	if len(dados) == 0 {
		return 0, nil
	}
	return s.pty.Write(dados)
}

// SemSuspender tira o Ctrl+Z (0x1a) da digitação.
func SemSuspender(dados []byte) []byte {
	if !bytes.Contains(dados, []byte{0x1a}) {
		return dados
	}
	limpo := make([]byte, 0, len(dados))
	for _, b := range dados {
		if b != 0x1a {
			limpo = append(limpo, b)
		}
	}
	return limpo
}

// pausaAntesDoEnter separa o texto do Enter: colados no mesmo instante, o
// programa pode ler o Enter como parte do texto.
const pausaAntesDoEnter = 80 * time.Millisecond

// Enviar escreve o texto como uma mensagem: entre as marcas de colagem, se o
// programa as pediu, e o Enter um instante depois.
func (s *Sessao) Enviar(texto string) error {
	if _, err := s.Escrever(Colagem(texto, s.ColagemLigada())); err != nil {
		return err
	}
	time.Sleep(pausaAntesDoEnter)
	_, err := s.Escrever([]byte("\r"))
	return err
}

// Variáveis que um Claude Code põe nos programas que ele abre. Se a Colmeia
// foi aberta de dentro de um Claude Code, o núcleo herda essas variáveis, e um
// agente que as recebesse se veria como sessão filha: não guardaria a conversa
// (não daria para retomar) e falaria com a sessão de fora pelo canal dela.
var variaveisDeSessao = map[string]bool{
	"CLAUDECODE":                   true,
	"AI_AGENT":                     true,
	"CLAUDE_PID":                   true,
	"CLAUDE_EFFORT":                true,
	"CLAUDE_CODE_CHILD_SESSION":    true,
	"CLAUDE_CODE_SESSION_ID":       true,
	"CLAUDE_CODE_SESSION_ATTENDED": true,
	"CLAUDE_CODE_ENTRYPOINT":       true,
	"CLAUDE_CODE_EXECPATH":         true,
	"CLAUDE_CODE_SSE_PORT":         true,
	"CLAUDE_CODE_MESSAGING_SOCKET": true,
	"CLAUDE_CODE_MESSAGING_TOKEN":  true,
}

// AmbienteLimpo tira do ambiente as variáveis de sessão de um Claude Code de
// fora. As de configuração (conta, provedor, modelo) continuam.
func AmbienteLimpo(ambiente []string) []string {
	limpo := make([]string, 0, len(ambiente))
	for _, v := range ambiente {
		nome, _, _ := strings.Cut(v, "=")
		if !variaveisDeSessao[nome] {
			limpo = append(limpo, v)
		}
	}
	return limpo
}

// Tamanho de um terminal em caracteres.
type Tamanho struct{ Colunas, Linhas uint16 }

// TamanhoPadrao é o tamanho de quando a tela não informa o dela.
var TamanhoPadrao = Tamanho{Colunas: 80, Linhas: 24}

// Valido diz se o tamanho é plausível.
func (t Tamanho) Valido() bool {
	return t.Colunas > 0 && t.Linhas > 0 && t.Colunas <= MaxColunas && t.Linhas <= MaxLinhas
}

// OuPadrao devolve o tamanho, ou o padrão se ele não for plausível.
func (t Tamanho) OuPadrao() Tamanho {
	if t.Valido() {
		return t
	}
	return TamanhoPadrao
}

// Redimensionar aceita só tamanhos plausíveis. O programa redesenha a tela
// depois, e isso também não conta como trabalho.
func (s *Sessao) Redimensionar(colunas, linhas uint16) {
	if !(Tamanho{colunas, linhas}).Valido() {
		return
	}
	s.marcarEntrada()
	s.mu.Lock()
	s.tela.redimensionar(Tamanho{colunas, linhas})
	s.mu.Unlock()
	s.pty.Redimensionar(colunas, linhas)
}

// Fechar encerra o programa e espera a leitura avisar o fim.
func (s *Sessao) Fechar() error {
	err := s.pty.Close()
	if s.lendo.Load() {
		select {
		case <-s.lido:
		case <-time.After(esperaLeitura):
		}
	}
	return err
}

// fecharPor marca quem da Colmeia encerrou, para o fim não parecer erro.
func (s *Sessao) fecharPor(motivo string) error {
	s.encerradaPor.CompareAndSwap(nil, motivo)
	if s.atividade != nil {
		s.atividade.congelar()
	}
	return s.Fechar()
}

// Fim avisa quando o programa do terminal termina.
func (s *Sessao) Fim() <-chan struct{} { return s.fim }

// Encerrada diz se o programa do terminal já terminou.
func (s *Sessao) Encerrada() bool {
	select {
	case <-s.fim:
		return true
	default:
		return false
	}
}

// Gerente guarda os terminais dos agentes, que aparecem e somem enquanto o
// núcleo roda.
type Gerente struct {
	mu      sync.Mutex
	sessoes map[int64]*Sessao
	// AoMudar recebe o início, as mudanças de estado e o fim de cada agente.
	// Não pode bloquear: é chamado de dentro da leitura dos terminais.
	AoMudar func(id int64, m Mudanca)
}

func NovoGerente() *Gerente { return &Gerente{sessoes: map[int64]*Sessao{}} }

// Adicionar registra a sessão e começa a ler dela. Se já havia uma com o mesmo
// id, a antiga é fechada.
func (g *Gerente) Adicionar(s *Sessao) {
	g.mu.Lock()
	antiga := g.sessoes[s.ID]
	g.sessoes[s.ID] = s
	ao := g.AoMudar
	g.mu.Unlock()
	if antiga != nil {
		antiga.fecharPor(PelaRemocao)
	}
	if ao != nil {
		s.aoMudar = func(m Mudanca) { ao(s.ID, m) }
	}
	if s.atividade != nil {
		s.avisar(Mudanca{Tipo: "iniciou", Estado: "trabalhando", Desde: time.Now()})
	}
	// Marcado antes de a goroutine começar: um Fechar logo em seguida espera o fim.
	s.lendo.Store(true)
	go s.Ler()
}

// Sessoes devolve as sessões abertas agora.
func (g *Gerente) Sessoes() []*Sessao {
	g.mu.Lock()
	defer g.mu.Unlock()
	lista := make([]*Sessao, 0, len(g.sessoes))
	for _, s := range g.sessoes {
		lista = append(lista, s)
	}
	return lista
}

func (g *Gerente) Pegar(id int64) (*Sessao, bool) {
	g.mu.Lock()
	defer g.mu.Unlock()
	s, ok := g.sessoes[id]
	return s, ok
}

// Ativa diz se há um terminal rodando para o id.
func (g *Gerente) Ativa(id int64) bool {
	s, ok := g.Pegar(id)
	return ok && !s.Encerrada()
}

// Parar encerra o programa do terminal do id, mas guarda o terminal (com o
// que foi escrito) para a tela continuar vendo.
func (g *Gerente) Parar(id int64) {
	if s, ok := g.Pegar(id); ok && !s.Encerrada() {
		s.fecharPor(PelaRemocao)
	}
}

// Fechar encerra e esquece o terminal do id, se houver.
func (g *Gerente) Fechar(id int64) {
	g.mu.Lock()
	s := g.sessoes[id]
	delete(g.sessoes, id)
	g.mu.Unlock()
	if s != nil {
		s.fecharPor(PelaRemocao)
	}
}

func (g *Gerente) Quantidade() int {
	g.mu.Lock()
	defer g.mu.Unlock()
	return len(g.sessoes)
}

// Rodando conta os terminais com o programa ainda aberto (Quantidade conta
// também os parados que a tela continua vendo).
func (g *Gerente) Rodando() int {
	g.mu.Lock()
	defer g.mu.Unlock()
	n := 0
	for _, s := range g.sessoes {
		if !s.Encerrada() {
			n++
		}
	}
	return n
}

// FecharTodos encerra todos os terminais, ao desligar o núcleo.
func (g *Gerente) FecharTodos() {
	g.mu.Lock()
	sessoes := g.sessoes
	g.sessoes = map[int64]*Sessao{}
	g.mu.Unlock()
	var espera sync.WaitGroup
	for _, s := range sessoes {
		espera.Go(func() { s.fecharPor(PeloDesligar) })
	}
	espera.Wait()
}

// UltimaEntrada diz quando a tela escreveu (ou mudou o tamanho) pela última
// vez; zero se nunca.
func (s *Sessao) UltimaEntrada() time.Time {
	if s.atividade == nil {
		return time.Time{}
	}
	if n := s.atividade.ultimaEntrada.Load(); n != 0 {
		return time.Unix(0, n)
	}
	return time.Time{}
}

// Colagem é o texto como a tela manda uma mensagem com várias linhas: entre
// as marcas de colagem (bracketed paste), para o programa não tratar cada
// quebra de linha como Enter. Quando o programa não ligou o modo de colagem,
// as marcas apareceriam cruas e cada linha viraria uma mensagem: as linhas
// vão juntas, separadas por espaço.
func Colagem(texto string, modoColagem bool) []byte {
	// Com o modo de colagem, qualquer texto vai entre as marcas: sem elas, o
	// Claude Code toma uma rajada de caracteres por colagem e o Enter que
	// vem logo atrás vira quebra de linha em vez de enviar.
	if modoColagem {
		return []byte("\x1b[200~" + texto + "\x1b[201~")
	}
	if !strings.ContainsAny(texto, "\r\n") {
		return []byte(texto)
	}
	linhas := strings.FieldsFunc(texto, func(r rune) bool { return r == '\n' || r == '\r' })
	for i := range linhas {
		linhas[i] = strings.TrimSpace(linhas[i])
	}
	return []byte(strings.Join(slices.DeleteFunc(linhas, func(l string) bool { return l == "" }), " "))
}
