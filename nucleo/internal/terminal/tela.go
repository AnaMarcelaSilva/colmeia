package terminal

import (
	"strings"
	"unicode"
	"unicode/utf8"

	"golang.org/x/text/width"
)

// tela é um emulador mínimo de terminal: uma grade de células onde a saída
// do programa é desenhada como um terminal de verdade faria. Serve para ler
// o que está na tela fora da Colmeia (a abelha fixa), porque as ferramentas
// novas desenham por posição (vá para a linha 12, coluna 40, escreva) e sem
// a grade o texto delas vira uma linha só, sem espaços.
//
// Entende o que o Claude Code e um shell usam: cursor (CUU/CUD/CUF/CUB/CHA/
// CUP/VPA), apagar (ED/EL/ECH/DCH/ICH), inserir e apagar linhas (IL/DL),
// região de rolagem (DECSTBM), tela alternativa (1049), salvar e voltar o
// cursor, e os caracteres largos (dois espaços na grade). Cores e títulos
// são ignorados.
type tela struct {
	colunas, linhas int
	grade           [][]rune // 0 é vazio; largo marca a segunda célula de um caractere largo
	linha, coluna   int
	quebraAdiada    bool // o cursor passou da última coluna: a próxima letra vai para a linha de baixo
	topo, base      int  // região de rolagem, inclusiva
	salvaLinha      int
	salvaColuna     int
	rolagem         [][]rune // o que saiu por cima na tela principal
	alternativa     [][]rune // a tela principal guardada enquanto a alternativa está ligada
	naAlternativa   bool
	resto           []byte // sequência ou caractere partido entre duas leituras
}

const (
	largo      rune = -1
	maxRolagem      = 400
	maxResto        = 4096
)

func novaTela(t Tamanho) *tela {
	t = t.OuPadrao()
	e := &tela{colunas: int(t.Colunas), linhas: int(t.Linhas)}
	e.grade = e.linhasVazias(e.linhas)
	e.base = e.linhas - 1
	return e
}

func (e *tela) linhasVazias(n int) [][]rune {
	g := make([][]rune, n)
	for i := range g {
		g[i] = make([]rune, e.colunas)
	}
	return g
}

// redimensionar muda a grade mantendo o canto de cima; se o cursor ficaria
// fora, as linhas de cima saem para a rolagem. O programa redesenha depois.
func (e *tela) redimensionar(t Tamanho) {
	if !t.Valido() || (int(t.Colunas) == e.colunas && int(t.Linhas) == e.linhas) {
		return
	}
	colunas, linhas := int(t.Colunas), int(t.Linhas)
	sobra := e.linha - (linhas - 1)
	if sobra > 0 {
		if !e.naAlternativa {
			e.rolagem = append(e.rolagem, e.grade[:sobra]...)
			e.limitarRolagem()
		}
		e.grade = e.grade[sobra:]
		e.linha -= sobra
	}
	e.colunas, e.linhas = colunas, linhas
	nova := e.linhasVazias(linhas)
	for i := 0; i < linhas && i < len(e.grade); i++ {
		copy(nova[i], e.grade[i])
	}
	e.grade = nova
	e.alternativa = nil
	e.topo, e.base = 0, linhas-1
	e.coluna = min(e.coluna, colunas-1)
	e.quebraAdiada = false
}

func (e *tela) limitarRolagem() {
	if len(e.rolagem) > maxRolagem {
		e.rolagem = append([][]rune(nil), e.rolagem[len(e.rolagem)-maxRolagem:]...)
	}
}

// texto devolve as últimas n linhas visíveis (rolagem e tela), sem os
// espaços do fim e sem as linhas vazias do fim.
func (e *tela) texto(n int) []string {
	var saida []string
	if !e.naAlternativa {
		for _, l := range e.rolagem {
			saida = append(saida, linhaTexto(l))
		}
	}
	for _, l := range e.grade {
		saida = append(saida, linhaTexto(l))
	}
	for len(saida) > 0 && saida[len(saida)-1] == "" {
		saida = saida[:len(saida)-1]
	}
	if len(saida) > n {
		saida = saida[len(saida)-n:]
	}
	return saida
}

func linhaTexto(l []rune) string {
	var b strings.Builder
	for _, r := range l {
		switch r {
		case 0:
			b.WriteByte(' ')
		case largo:
		default:
			b.WriteRune(r)
		}
	}
	return strings.TrimRight(b.String(), " ")
}

// alimentar desenha mais saída do programa na grade.
func (e *tela) alimentar(b []byte) {
	if len(e.resto) > 0 {
		b = append(append([]byte(nil), e.resto...), b...)
		e.resto = e.resto[:0]
	}
	i := 0
	for i < len(b) {
		c := b[i]
		switch {
		case c == 0x1b:
			fim, completa := e.sequencia(b[i:])
			if !completa {
				if len(b)-i <= maxResto {
					e.resto = append(e.resto[:0], b[i:]...)
				}
				return
			}
			i += fim
		case c < 0x20 || c == 0x7f:
			e.controle(c)
			i++
		default:
			if !utf8.FullRune(b[i:]) {
				e.resto = append(e.resto[:0], b[i:]...)
				return
			}
			r, n := utf8.DecodeRune(b[i:])
			i += n
			if r != utf8.RuneError {
				e.escrever(r)
			}
		}
	}
}

func (e *tela) controle(c byte) {
	switch c {
	case '\n', '\v', '\f':
		e.descer()
	case '\r':
		e.coluna, e.quebraAdiada = 0, false
	case '\b':
		if e.coluna > 0 {
			e.coluna--
		}
		e.quebraAdiada = false
	case '\t':
		e.coluna = min((e.coluna/8+1)*8, e.colunas-1)
		e.quebraAdiada = false
	}
}

// sequencia interpreta uma sequência de escape no começo de b e devolve o
// tamanho dela; completa é falso quando ela ainda não chegou inteira.
func (e *tela) sequencia(b []byte) (int, bool) {
	if len(b) < 2 {
		return 0, false
	}
	switch b[1] {
	case '[':
		return e.csi(b)
	case ']', 'P', 'X', '^', '_': // OSC, DCS, SOS, PM, APC: até BEL ou ESC \
		for i := 2; i < len(b); i++ {
			if b[i] == 0x07 {
				return i + 1, true
			}
			if b[i] == 0x1b {
				if i+1 >= len(b) {
					return 0, false
				}
				return i + 2, true
			}
		}
		return 0, false
	case '(', ')', '*', '+', '#', '%': // conjunto de caracteres: mais um byte
		if len(b) < 3 {
			return 0, false
		}
		return 3, true
	case '7':
		e.salvaLinha, e.salvaColuna = e.linha, e.coluna
	case '8':
		e.mover(e.salvaLinha, e.salvaColuna)
	case 'D':
		e.descer()
	case 'M':
		e.subir()
	case 'E':
		e.descer()
		e.coluna = 0
	case 'c':
		e.limparTudo()
		e.mover(0, 0)
		e.topo, e.base = 0, e.linhas-1
	}
	return 2, true
}

func (e *tela) csi(b []byte) (int, bool) {
	i := 2
	privado := byte(0)
	if i < len(b) && (b[i] == '?' || b[i] == '>' || b[i] == '<' || b[i] == '=' || b[i] == '!') {
		privado = b[i]
		i++
	}
	inicio := i
	for i < len(b) && (b[i] >= '0' && b[i] <= '9' || b[i] == ';' || b[i] == ':') {
		i++
	}
	params := string(b[inicio:i])
	for i < len(b) && b[i] >= 0x20 && b[i] <= 0x2f {
		i++ // intermediários
	}
	if i >= len(b) {
		return 0, false
	}
	final := b[i]
	if final < 0x40 || final > 0x7e {
		return i + 1, true
	}
	e.comando(final, privado, numeros(params))
	return i + 1, true
}

func numeros(params string) []int {
	if params == "" {
		return nil
	}
	partes := strings.Split(params, ";")
	n := make([]int, len(partes))
	for i, p := range partes {
		if j := strings.IndexByte(p, ':'); j >= 0 {
			p = p[:j]
		}
		v := 0
		for _, d := range p {
			if d >= '0' && d <= '9' && v < 100000 {
				v = v*10 + int(d-'0')
			}
		}
		n[i] = v
	}
	return n
}

func param(n []int, i, padrao int) int {
	if i < len(n) && n[i] > 0 {
		return n[i]
	}
	return padrao
}

func (e *tela) comando(final, privado byte, n []int) {
	if privado == '?' {
		if final == 'h' || final == 'l' {
			for _, m := range n {
				if m == 1049 || m == 1047 || m == 47 {
					e.alternar(final == 'h', m == 1049)
				}
			}
		}
		return
	}
	if privado != 0 {
		return
	}
	switch final {
	case 'A':
		e.mover(e.linha-param(n, 0, 1), e.coluna)
	case 'B', 'e':
		e.mover(e.linha+param(n, 0, 1), e.coluna)
	case 'C', 'a':
		e.mover(e.linha, e.coluna+param(n, 0, 1))
	case 'D':
		e.mover(e.linha, e.coluna-param(n, 0, 1))
	case 'E':
		e.mover(e.linha+param(n, 0, 1), 0)
	case 'F':
		e.mover(e.linha-param(n, 0, 1), 0)
	case 'G', '`':
		e.mover(e.linha, param(n, 0, 1)-1)
	case 'd':
		e.mover(param(n, 0, 1)-1, e.coluna)
	case 'H', 'f':
		e.mover(param(n, 0, 1)-1, param(n, 1, 1)-1)
	case 'J':
		e.apagarTela(param(n, 0, 0))
	case 'K':
		e.apagarLinha(param(n, 0, 0))
	case 'L':
		e.inserirLinhas(param(n, 0, 1))
	case 'M':
		e.apagarLinhas(param(n, 0, 1))
	case '@':
		e.inserirCelulas(param(n, 0, 1))
	case 'P':
		e.apagarCelulas(param(n, 0, 1))
	case 'X':
		e.limparCelulas(param(n, 0, 1))
	case 'S':
		for range param(n, 0, 1) {
			e.rolarCima()
		}
	case 'T':
		for range param(n, 0, 1) {
			e.rolarBaixo()
		}
	case 'r':
		topo, base := param(n, 0, 1)-1, param(n, 1, e.linhas)-1
		if topo >= 0 && base < e.linhas && topo < base {
			e.topo, e.base = topo, base
			e.mover(0, 0)
		}
	case 's':
		e.salvaLinha, e.salvaColuna = e.linha, e.coluna
	case 'u':
		e.mover(e.salvaLinha, e.salvaColuna)
	}
}

func (e *tela) mover(linha, coluna int) {
	e.linha = max(0, min(linha, e.linhas-1))
	e.coluna = max(0, min(coluna, e.colunas-1))
	e.quebraAdiada = false
}

func (e *tela) alternar(ligar, limpar bool) {
	if ligar == e.naAlternativa {
		return
	}
	if ligar {
		e.alternativa = e.grade
		e.grade = e.linhasVazias(e.linhas)
		e.salvaLinha, e.salvaColuna = e.linha, e.coluna
	} else {
		if e.alternativa != nil {
			e.grade = e.alternativa
		}
		e.alternativa = nil
		if limpar {
			e.mover(e.salvaLinha, e.salvaColuna)
		}
	}
	e.naAlternativa = ligar
	e.quebraAdiada = false
}

// descer é o avanço de linha: no fim da região, a tela rola.
func (e *tela) descer() {
	e.quebraAdiada = false
	if e.linha == e.base {
		e.rolarCima()
		return
	}
	if e.linha < e.linhas-1 {
		e.linha++
	}
}

func (e *tela) subir() {
	e.quebraAdiada = false
	if e.linha == e.topo {
		e.rolarBaixo()
		return
	}
	if e.linha > 0 {
		e.linha--
	}
}

// rolarCima tira a linha de cima da região (para a rolagem, se a região é a
// tela inteira da principal) e abre uma vazia embaixo.
func (e *tela) rolarCima() {
	if e.topo == 0 && !e.naAlternativa {
		e.rolagem = append(e.rolagem, e.grade[0])
		e.limitarRolagem()
	}
	copy(e.grade[e.topo:e.base], e.grade[e.topo+1:e.base+1])
	e.grade[e.base] = make([]rune, e.colunas)
}

func (e *tela) rolarBaixo() {
	copy(e.grade[e.topo+1:e.base+1], e.grade[e.topo:e.base])
	e.grade[e.topo] = make([]rune, e.colunas)
}

func (e *tela) escrever(r rune) {
	w := largura(r)
	if w == 0 {
		return
	}
	if e.quebraAdiada {
		e.coluna = 0
		e.descer()
	}
	if w == 2 && e.coluna == e.colunas-1 {
		e.grade[e.linha][e.coluna] = 0
		e.coluna = 0
		e.descer()
	}
	l := e.grade[e.linha]
	// Escrever em cima da metade de um largo apaga a outra metade.
	if l[e.coluna] == largo && e.coluna > 0 {
		l[e.coluna-1] = 0
	}
	if e.coluna+1 < e.colunas && l[e.coluna+1] == largo && l[e.coluna] != largo {
		l[e.coluna+1] = 0
	}
	l[e.coluna] = r
	if w == 2 {
		l[e.coluna+1] = largo
	}
	e.coluna += w
	if e.coluna >= e.colunas {
		e.coluna = e.colunas - 1
		e.quebraAdiada = true
	}
}

// largura é quantas células o caractere ocupa: dois para os largos (ideogramas,
// emojis), zero para acentos combinados e marcas invisíveis.
func largura(r rune) int {
	if unicode.Is(unicode.Mn, r) || unicode.Is(unicode.Me, r) || unicode.Is(unicode.Cf, r) || r >= 0xfe00 && r <= 0xfe0f {
		return 0
	}
	switch width.LookupRune(r).Kind() {
	case width.EastAsianWide, width.EastAsianFullwidth:
		return 2
	}
	if r >= 0x1f300 && r <= 0x1faff {
		return 2
	}
	return 1
}

func (e *tela) apagarTela(modo int) {
	switch modo {
	case 0:
		e.apagarLinha(0)
		for i := e.linha + 1; i < e.linhas; i++ {
			e.grade[i] = make([]rune, e.colunas)
		}
	case 1:
		e.apagarLinha(1)
		for i := 0; i < e.linha; i++ {
			e.grade[i] = make([]rune, e.colunas)
		}
	case 2, 3:
		e.limparTudo()
	}
}

func (e *tela) limparTudo() {
	e.grade = e.linhasVazias(e.linhas)
	e.quebraAdiada = false
}

func (e *tela) apagarLinha(modo int) {
	l := e.grade[e.linha]
	switch modo {
	case 0:
		for i := e.coluna; i < e.colunas; i++ {
			l[i] = 0
		}
	case 1:
		for i := 0; i <= e.coluna; i++ {
			l[i] = 0
		}
	case 2:
		for i := range l {
			l[i] = 0
		}
	}
	e.quebraAdiada = false
}

func (e *tela) inserirLinhas(n int) {
	if e.linha < e.topo || e.linha > e.base {
		return
	}
	for range min(n, e.base-e.linha+1) {
		copy(e.grade[e.linha+1:e.base+1], e.grade[e.linha:e.base])
		e.grade[e.linha] = make([]rune, e.colunas)
	}
	e.coluna, e.quebraAdiada = 0, false
}

func (e *tela) apagarLinhas(n int) {
	if e.linha < e.topo || e.linha > e.base {
		return
	}
	for range min(n, e.base-e.linha+1) {
		copy(e.grade[e.linha:e.base], e.grade[e.linha+1:e.base+1])
		e.grade[e.base] = make([]rune, e.colunas)
	}
	e.coluna, e.quebraAdiada = 0, false
}

func (e *tela) inserirCelulas(n int) {
	l := e.grade[e.linha]
	n = min(n, e.colunas-e.coluna)
	copy(l[e.coluna+n:], l[e.coluna:e.colunas-n])
	for i := e.coluna; i < e.coluna+n; i++ {
		l[i] = 0
	}
	e.quebraAdiada = false
}

func (e *tela) apagarCelulas(n int) {
	l := e.grade[e.linha]
	n = min(n, e.colunas-e.coluna)
	copy(l[e.coluna:], l[e.coluna+n:])
	for i := e.colunas - n; i < e.colunas; i++ {
		l[i] = 0
	}
	e.quebraAdiada = false
}

func (e *tela) limparCelulas(n int) {
	l := e.grade[e.linha]
	for i := e.coluna; i < min(e.coluna+n, e.colunas); i++ {
		l[i] = 0
	}
	e.quebraAdiada = false
}
