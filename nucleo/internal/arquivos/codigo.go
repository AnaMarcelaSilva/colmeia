package arquivos

// A aba Arquivos da tarefa: busca por nome, busca por texto e o que o agente
// mudou contra a branch base. Só leitura, sempre dentro da pasta da tarefa,
// com limite de resultados e de tamanho; o git roda sem shell (pacote git).

import (
	"context"
	"errors"
	"io"
	"io/fs"
	"path/filepath"
	"slices"
	"strconv"
	"strings"
	"unicode"
	"unicode/utf8"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/git"
)

const (
	// MaxNomes devolvidos pela busca por nome.
	MaxNomes = 50
	// MaxAchados devolvidos pela busca por texto.
	MaxAchados = 200
	// MaxMudancas listadas.
	MaxMudancas = 500
	// MaxTermo em caracteres.
	MaxTermo = 200
	// MaxTrecho: a linha achada é cortada em volta do termo.
	MaxTrecho = 240
	// maxArquivos olhados pela busca por nome.
	maxArquivos = 50000
)

var (
	ErrTermo    = errors.New("o termo da busca precisa ter de 1 a 200 caracteres, sem quebra de linha")
	ErrTermoCur = errors.New("digite pelo menos 2 caracteres para buscar no código")
	ErrSemGit   = errors.New("a pasta da tarefa não é um repositório git")
	ErrSemBase  = errors.New("não achei a branch base (main ou master) para comparar")
)

func termoValido(termo string, minimo int) (string, error) {
	if !utf8.ValidString(termo) || strings.ContainsFunc(termo, unicode.IsControl) || utf8.RuneCountInString(termo) > MaxTermo {
		return "", ErrTermo
	}
	if utf8.RuneCountInString(strings.TrimSpace(termo)) < minimo {
		if minimo > 1 {
			return "", ErrTermoCur
		}
		return "", ErrTermo
	}
	return termo, nil
}

// emPastaIgnorada diz se o caminho (relativo, com "/") passa por uma das
// pastas pesadas, que não entram nos resultados.
func emPastaIgnorada(caminho string) bool {
	for parte := range strings.SplitSeq(caminho, "/") {
		if Ignoradas[parte] {
			return true
		}
	}
	return false
}

func traduzirGit(err error) error {
	switch {
	case errors.Is(err, git.ErrNaoRepositorio):
		return ErrSemGit
	case errors.Is(err, git.ErrSemBase):
		return ErrSemBase
	}
	return err
}

// Nome é um arquivo achado pela busca por nome.
type Nome struct {
	Caminho string `json:"caminho"`
}

// pontuar dá a nota de um caminho para o termo (minúsculo): 0 não serve.
// Nome do arquivo que começa com o termo vale mais que nome que contém, que
// vale mais que caminho que contém, que vale mais que as letras em ordem.
func pontuar(caminho, termo string) int {
	c := strings.ToLower(caminho)
	nome := c[strings.LastIndexByte(c, '/')+1:]
	switch {
	case strings.HasPrefix(nome, termo):
		return 400 - min(len(nome), 100)
	case strings.Contains(nome, termo):
		return 300 - min(len(nome), 100)
	case strings.Contains(c, termo):
		return 200 - min(len(c), 100)
	}
	// As letras do termo em ordem no caminho (como o Ctrl+P dos editores).
	i := 0
	for _, r := range c {
		if i < len(termo) {
			t, n := utf8.DecodeRuneInString(termo[i:])
			if r == t {
				i += n
			}
		}
	}
	if i == len(termo) {
		return 100 - min(len(c), 99)
	}
	return 0
}

// BuscarNome acha arquivos da pasta pelo nome. Com git, olha o que ele
// conhece e os novos não ignorados; sem git, percorre a pasta pulando as
// pastas pesadas. Devolve os melhores, até MaxNomes.
func BuscarNome(ctx context.Context, pasta, termo string) (nomes []Nome, mais bool, err error) {
	termo, err = termoValido(termo, 1)
	if err != nil {
		return nil, false, err
	}
	termo = strings.ToLower(strings.TrimSpace(termo))
	caminhos, cortado, err := git.ListarArquivos(ctx, pasta, maxArquivos)
	if errors.Is(err, git.ErrNaoRepositorio) {
		caminhos, cortado, err = percorrer(pasta, maxArquivos)
	}
	if err != nil {
		return nil, false, traduzirGit(err)
	}
	type nota struct {
		caminho string
		valor   int
	}
	var notas []nota
	for _, c := range caminhos {
		if emPastaIgnorada(c) {
			continue
		}
		if v := pontuar(c, termo); v > 0 {
			notas = append(notas, nota{c, v})
		}
	}
	slices.SortStableFunc(notas, func(a, b nota) int {
		if a.valor != b.valor {
			return b.valor - a.valor
		}
		return strings.Compare(a.caminho, b.caminho)
	})
	mais = cortado || len(notas) > MaxNomes
	nomes = make([]Nome, 0, min(len(notas), MaxNomes))
	for _, n := range notas[:min(len(notas), MaxNomes)] {
		nomes = append(nomes, Nome{Caminho: n.caminho})
	}
	return nomes, mais, nil
}

// percorrer lista os arquivos de uma pasta sem git, dentro dela (os.Root),
// sem seguir links e sem entrar nas pastas pesadas.
func percorrer(pasta string, max int) ([]string, bool, error) {
	raiz, err := abrir(pasta)
	if err != nil {
		return nil, false, err
	}
	defer raiz.Close()
	var lista []string
	cortado := false
	err = fs.WalkDir(raiz.FS(), ".", func(caminho string, d fs.DirEntry, err error) error {
		if err != nil {
			return nil
		}
		if d.IsDir() {
			if Ignoradas[d.Name()] {
				return fs.SkipDir
			}
			return nil
		}
		if !d.Type().IsRegular() {
			return nil
		}
		if len(lista) == max {
			cortado = true
			return fs.SkipAll
		}
		lista = append(lista, caminho)
		return nil
	})
	return lista, cortado, err
}

// BuscarTexto procura o termo (texto fixo) nos arquivos de texto da pasta
// e devolve a linha e o trecho de cada achado, até MaxAchados. Arquivos que
// costumam guardar senha ficam de fora.
func BuscarTexto(ctx context.Context, pasta, termo string, maiusculas bool) (achados []git.Achado, mais bool, err error) {
	termo, err = termoValido(termo, 2)
	if err != nil {
		return nil, false, err
	}
	brutos, mais, err := git.Grep(ctx, pasta, termo, maiusculas, MaxAchados*2)
	if err != nil {
		return nil, false, traduzirGit(err)
	}
	for _, a := range brutos {
		// O conteúdo de um arquivo que costuma ter senha não aparece no
		// trecho (a tela pode estar compartilhada).
		if emPastaIgnorada(a.Caminho) || Sensivel(filepath.Base(a.Caminho)) {
			continue
		}
		if len(achados) == MaxAchados {
			return achados, true, nil
		}
		a.Texto = trecho(a.Texto, termo, maiusculas)
		achados = append(achados, a)
	}
	return achados, mais, nil
}

// trecho corta a linha em volta do termo, com no máximo MaxTrecho runas,
// tabs viram espaços e o começo em branco sai.
func trecho(linha, termo string, maiusculas bool) string {
	linha = strings.ToValidUTF8(strings.ReplaceAll(linha, "\t", "    "), "�")
	linha = strings.TrimLeft(linha, " ")
	runas := []rune(linha)
	if len(runas) <= MaxTrecho {
		return linha
	}
	alvo, t := linha, termo
	if !maiusculas {
		alvo, t = strings.ToLower(linha), strings.ToLower(termo)
	}
	inicio := 0
	if i := strings.Index(alvo, t); i >= 0 {
		inicio = max(utf8.RuneCountInString(alvo[:i])-MaxTrecho/3, 0)
	}
	fim := min(inicio+MaxTrecho, len(runas))
	saida := string(runas[inicio:fim])
	if inicio > 0 {
		saida = "…" + saida
	}
	if fim < len(runas) {
		saida += "…"
	}
	return saida
}

// Mudancas é o que mudou na pasta da tarefa contra a branch base.
type Mudancas struct {
	Base     string        `json:"base"`
	Arquivos []git.Mudanca `json:"arquivos"`
	Mais     bool          `json:"mais"`
}

// ListarMudancas compara a pasta da tarefa (com o que ainda não está em
// commit) com o ponto em que ela saiu da branch base.
func ListarMudancas(ctx context.Context, pasta string) (Mudancas, error) {
	nome, commit, err := git.Base(ctx, pasta)
	if err != nil {
		return Mudancas{}, traduzirGit(err)
	}
	lista, mais, err := git.Mudancas(ctx, pasta, commit, MaxMudancas)
	if err != nil {
		return Mudancas{}, traduzirGit(err)
	}
	m := Mudancas{Base: nome, Arquivos: make([]git.Mudanca, 0, len(lista)), Mais: mais}
	for _, a := range lista {
		if !emPastaIgnorada(a.Caminho) {
			m.Arquivos = append(m.Arquivos, a)
		}
	}
	return m, nil
}

// LinhaDiff é uma linha do diff: trecho ("@@"), igual, mais ou menos, com
// o número da linha antes (Antiga) e depois (Nova); 0 quando não tem.
type LinhaDiff struct {
	Tipo   string `json:"tipo"`
	Antiga int    `json:"antiga,omitempty"`
	Nova   int    `json:"nova,omitempty"`
	Texto  string `json:"texto"`
}

// Diferenca é o diff de um arquivo contra a base.
type Diferenca struct {
	Tipo     string      `json:"tipo"` // diff, binario, sensivel ou igual
	Caminho  string      `json:"caminho"`
	Base     string      `json:"base"`
	Linhas   []LinhaDiff `json:"linhas"`
	Cortado  bool        `json:"cortado,omitempty"`
	Absoluto string      `json:"caminho_absoluto"`
}

// VerDiferenca monta o diff de um arquivo da pasta contra a base. Um
// arquivo novo fora do git sai inteiro como "mais"; um sensível só vem com
// o conteúdo se mostrar for true.
func VerDiferenca(ctx context.Context, pasta, caminho string, mostrar bool) (Diferenca, error) {
	rel, err := Relativo(pasta, caminho)
	if err != nil {
		return Diferenca{}, err
	}
	if rel == "." {
		return Diferenca{}, ErrNaoArquivo
	}
	nome, commit, err := git.Base(ctx, pasta)
	if err != nil {
		return Diferenca{}, traduzirGit(err)
	}
	barra := filepath.ToSlash(rel)
	d := Diferenca{Caminho: barra, Base: nome, Linhas: []LinhaDiff{}, Absoluto: filepath.Join(pasta, rel)}
	if Sensivel(filepath.Base(rel)) && !mostrar {
		d.Tipo = "sensivel"
		return d, nil
	}
	texto, cortado, err := git.Diff(ctx, pasta, commit, barra, MaxTexto)
	if err != nil {
		return Diferenca{}, traduzirGit(err)
	}
	if texto == "" {
		return novoInteiro(ctx, pasta, rel, d)
	}
	d.Tipo, d.Cortado = "diff", cortado
	d.Linhas = lerDiff(texto, cortado)
	if len(d.Linhas) == 0 && strings.Contains(texto, "Binary files") {
		d.Tipo = "binario"
	}
	return d, nil
}

// novoInteiro: sem diff, o arquivo é novo fora do git (sai inteiro) ou
// está igual à base.
func novoInteiro(ctx context.Context, pasta, rel string, d Diferenca) (Diferenca, error) {
	novo, err := git.Novo(ctx, pasta, filepath.ToSlash(rel))
	if err != nil || !novo {
		d.Tipo = "igual"
		return d, nil
	}
	f, _, info, err := abrirArquivo(pasta, rel)
	if err != nil {
		return Diferenca{}, err
	}
	defer f.Close()
	bruto, err := io.ReadAll(io.LimitReader(f, MaxTexto))
	if err != nil {
		return Diferenca{}, traduzir(err, ErrNaoExiste)
	}
	if slices.Contains(bruto[:min(len(bruto), olharBinario)], 0) {
		d.Tipo = "binario"
		return d, nil
	}
	d.Tipo, d.Cortado = "diff", info.Size() > int64(len(bruto))
	texto := strings.TrimSuffix(strings.ToValidUTF8(string(bruto), "�"), "\n")
	linhas := strings.Split(texto, "\n")
	d.Linhas = append(d.Linhas, LinhaDiff{Tipo: "trecho", Texto: "@@ -0,0 +1," + strconv.Itoa(len(linhas)) + " @@ arquivo novo"})
	for i, l := range linhas {
		d.Linhas = append(d.Linhas, LinhaDiff{Tipo: "mais", Nova: i + 1, Texto: l})
	}
	return d, nil
}

// lerDiff transforma o diff unificado de um arquivo nas linhas da tela,
// com os números antigos e novos. O cabeçalho (diff, index, ---, +++) sai.
func lerDiff(texto string, cortado bool) []LinhaDiff {
	var linhas []LinhaDiff
	antiga, nova := 0, 0
	dentro := false
	partes := strings.Split(texto, "\n")
	if cortado && len(partes) > 0 {
		partes = partes[:len(partes)-1] // a última pode ter sido cortada no meio
	}
	for _, l := range partes {
		switch {
		case strings.HasPrefix(l, "@@"):
			dentro = true
			antiga, nova = inicioDoTrecho(l)
			linhas = append(linhas, LinhaDiff{Tipo: "trecho", Texto: l})
		case !dentro:
			continue
		case strings.HasPrefix(l, "+"):
			linhas = append(linhas, LinhaDiff{Tipo: "mais", Nova: nova, Texto: l[1:]})
			nova++
		case strings.HasPrefix(l, "-"):
			linhas = append(linhas, LinhaDiff{Tipo: "menos", Antiga: antiga, Texto: l[1:]})
			antiga++
		case strings.HasPrefix(l, " "):
			linhas = append(linhas, LinhaDiff{Tipo: "igual", Antiga: antiga, Nova: nova, Texto: l[1:]})
			antiga++
			nova++
		}
	}
	for i := range linhas {
		linhas[i].Texto = strings.ToValidUTF8(strings.ReplaceAll(linhas[i].Texto, "\t", "    "), "�")
	}
	return linhas
}

// inicioDoTrecho lê "@@ -12,5 +12,7 @@": as linhas onde o trecho começa.
func inicioDoTrecho(l string) (antiga, nova int) {
	campos := strings.Fields(l)
	if len(campos) < 3 {
		return 0, 0
	}
	ler := func(s string) int {
		s = strings.TrimLeft(s, "-+")
		n, _ := strconv.Atoi(strings.SplitN(s, ",", 2)[0])
		return n
	}
	return ler(campos[1]), ler(campos[2])
}

// ErroDeCodigo diz se o erro é da busca ou do diff, com mensagem para mostrar.
func ErroDeCodigo(err error) bool {
	for _, e := range []error{ErrTermo, ErrTermoCur, ErrSemGit, ErrSemBase} {
		if errors.Is(err, e) {
			return true
		}
	}
	return false
}
