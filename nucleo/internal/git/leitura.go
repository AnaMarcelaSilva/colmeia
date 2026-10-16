package git

import (
	"bufio"
	"bytes"
	"context"
	"errors"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strconv"
	"strings"
	"time"
)

// Consultas só de leitura na pasta de uma tarefa (a aba Arquivos): a lista
// de arquivos, a busca por texto e o que mudou contra a branch base. A saída
// é lida aos pedaços e cortada no limite: um repositório grande não enche a
// memória nem segura a resposta.

// limiteLeitura é o tempo máximo de uma consulta da aba Arquivos.
const limiteLeitura = 8 * time.Second

// ErrSemBase diz que não há branch base para comparar (nem main nem master).
var ErrSemBase = errors.New("não achei a branch base (main ou master) para comparar")

// lerLimitado roda o git e lê no máximo maxBytes da saída; cortado diz que
// havia mais. Nada passa por shell, nenhum programa externo de diff ou de
// conversão roda (o repositório pode ter configurado um) e o índice não é
// regravado.
func lerLimitado(ctx context.Context, dir string, maxBytes int, args ...string) (saida []byte, cortado bool, err error) {
	ctx, cancelar := context.WithTimeout(ctx, limiteLeitura)
	defer cancelar()
	globais := []string{"--literal-pathspecs", "-c", "core.fsmonitor=false", "-c", "core.quotepath=off", "-C", dir}
	cmd := exec.CommandContext(ctx, "git", append(globais, args...)...)
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0", "GIT_EDITOR=true", "GIT_OPTIONAL_LOCKS=0", "GIT_PAGER=cat", "LC_ALL=C")
	var erros bytes.Buffer
	cmd.Stderr = &erros
	tubo, err := cmd.StdoutPipe()
	if err != nil {
		return nil, false, err
	}
	if err := cmd.Start(); err != nil {
		return nil, false, err
	}
	saida, _ = io.ReadAll(io.LimitReader(tubo, int64(maxBytes)+1))
	if len(saida) > maxBytes {
		saida, cortado = saida[:maxBytes], true
		// O resto não interessa: o processo sai no próximo write.
		cmd.Process.Kill()
	}
	io.Copy(io.Discard, tubo)
	err = cmd.Wait()
	if cortado {
		return saida, true, nil
	}
	if ctx.Err() != nil {
		return saida, true, nil
	}
	if err != nil {
		var falha *exec.ExitError
		if errors.As(err, &falha) {
			// git grep sai com 1 quando não acha nada.
			if falha.ExitCode() == 1 && strings.TrimSpace(erros.String()) == "" {
				return nil, false, nil
			}
			msg := strings.TrimSpace(erros.String())
			if strings.Contains(msg, "not a git repository") {
				return nil, false, ErrNaoRepositorio
			}
			return nil, false, fmt.Errorf("%s", msg)
		}
		return nil, false, err
	}
	return saida, false, nil
}

// separarNUL quebra uma saída -z; um último pedaço sem o NUL final (corte) sai.
func separarNUL(saida []byte, cortado bool) []string {
	partes := strings.Split(string(saida), "\x00")
	if len(partes) > 0 && (partes[len(partes)-1] == "" || cortado) {
		partes = partes[:len(partes)-1]
	}
	return partes
}

// ListarArquivos devolve os arquivos da pasta que o git conhece ou que são
// novos e não ignorados, relativos à pasta, até max.
func ListarArquivos(ctx context.Context, dir string, max int) (lista []string, mais bool, err error) {
	saida, cortado, err := lerLimitado(ctx, dir, max*256, "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--deduplicate")
	if err != nil {
		return nil, false, err
	}
	lista = separarNUL(saida, cortado)
	if len(lista) > max {
		lista, cortado = lista[:max], true
	}
	return lista, cortado, nil
}

// Achado é uma linha que a busca por texto encontrou.
type Achado struct {
	Caminho string `json:"caminho"`
	Linha   int    `json:"linha"`
	Texto   string `json:"texto"`
}

// Grep procura o termo (texto fixo, não expressão) nos arquivos de texto da
// pasta, versionados ou novos, e devolve até max linhas.
func Grep(ctx context.Context, dir, termo string, maiusculas bool, max int) (achados []Achado, mais bool, err error) {
	args := []string{"grep", "-z", "-n", "-I", "--no-color", "--no-textconv", "--untracked", "-F"}
	if !maiusculas {
		args = append(args, "-i")
	}
	// -e: um termo que começa com "-" não vira opção.
	args = append(args, "-e", termo)
	saida, cortado, err := lerLimitado(ctx, dir, 2<<20, args...)
	if err != nil {
		return nil, false, err
	}
	leitor := bufio.NewScanner(bytes.NewReader(saida))
	leitor.Buffer(make([]byte, 64<<10), 2<<20)
	for leitor.Scan() {
		// Com -z: caminho NUL linha NUL texto.
		partes := strings.SplitN(leitor.Text(), "\x00", 3)
		if len(partes) != 3 {
			continue
		}
		n, err := strconv.Atoi(partes[1])
		if err != nil {
			continue
		}
		if len(achados) == max {
			return achados, true, nil
		}
		achados = append(achados, Achado{Caminho: partes[0], Linha: n, Texto: partes[2]})
	}
	return achados, cortado, nil
}

// Base acha a branch com que a tarefa se compara (a padrão do remoto, main
// ou master) e o ponto em que a branch atual saiu dela.
func Base(ctx context.Context, dir string) (nome, commit string, err error) {
	candidatas := []string{}
	if padrao, err := rodar(ctx, dir, "symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"); err == nil && padrao != "" {
		candidatas = append(candidatas, padrao)
	}
	candidatas = append(candidatas, "main", "master", "origin/main", "origin/master")
	for _, c := range candidatas {
		if _, err := rodar(ctx, dir, "rev-parse", "-q", "--verify", c+"^{commit}"); err != nil {
			continue
		}
		commit, err := rodar(ctx, dir, "merge-base", "HEAD", c)
		if err != nil {
			continue
		}
		return c, commit, nil
	}
	if _, err := rodar(ctx, dir, "rev-parse", "--show-toplevel"); err != nil {
		return "", "", ErrNaoRepositorio
	}
	return "", "", ErrSemBase
}

// Mudanca é um arquivo diferente da base: A (novo), M, D (apagado).
type Mudanca struct {
	Caminho string `json:"caminho"`
	Estado  string `json:"estado"`
	Mais    int    `json:"mais"`
	Menos   int    `json:"menos"`
	Binario bool   `json:"binario,omitempty"`
}

// Mudancas compara a pasta (com o que ainda não está em commit) com o
// commit base. Os arquivos novos fora do git entram como A, sem contagem.
func Mudancas(ctx context.Context, dir, base string, max int) (lista []Mudanca, mais bool, err error) {
	estados := map[string]string{}
	saida, cortado, err := lerLimitado(ctx, dir, max*256, "diff", "--name-status", "-z", "--no-renames", "--relative", "--no-ext-diff", base)
	if err != nil {
		return nil, false, err
	}
	partes := separarNUL(saida, cortado)
	for i := 0; i+1 < len(partes); i += 2 {
		estados[partes[i+1]] = partes[i][:1]
	}
	saida, cortado2, err := lerLimitado(ctx, dir, max*256, "diff", "--numstat", "-z", "--no-renames", "--relative", "--no-ext-diff", "--no-textconv", base)
	if err != nil {
		return nil, false, err
	}
	for _, linha := range separarNUL(saida, cortado2) {
		// Com -z: "mais\tmenos\tcaminho".
		campos := strings.SplitN(linha, "\t", 3)
		if len(campos) != 3 {
			continue
		}
		m := Mudanca{Caminho: campos[2], Estado: estados[campos[2]]}
		if m.Estado == "" {
			m.Estado = "M"
		}
		if campos[0] == "-" {
			m.Binario = true
		} else {
			m.Mais, _ = strconv.Atoi(campos[0])
			m.Menos, _ = strconv.Atoi(campos[1])
		}
		lista = append(lista, m)
	}
	saida, cortado3, err := lerLimitado(ctx, dir, max*256, "ls-files", "-z", "--others", "--exclude-standard")
	if err != nil {
		return nil, false, err
	}
	for _, c := range separarNUL(saida, cortado3) {
		lista = append(lista, Mudanca{Caminho: c, Estado: "A"})
	}
	mais = cortado || cortado2 || cortado3
	if len(lista) > max {
		lista, mais = lista[:max], true
	}
	return lista, mais, nil
}

// Diff devolve o diff unificado de um arquivo (caminho já validado e
// relativo à pasta) contra o commit base, até maxBytes.
func Diff(ctx context.Context, dir, base, caminho string, maxBytes int) (texto string, cortado bool, err error) {
	saida, cortado, err := lerLimitado(ctx, dir, maxBytes, "diff", "--no-color", "--no-ext-diff", "--no-textconv", "--no-renames", "--relative", "-U3", base, "--", caminho)
	return string(saida), cortado, err
}

// Novo diz se o arquivo (já validado, relativo à pasta) é novo fora do git
// e não ignorado.
func Novo(ctx context.Context, dir, caminho string) (bool, error) {
	saida, _, err := lerLimitado(ctx, dir, 64<<10, "ls-files", "-z", "--others", "--exclude-standard", "--", caminho)
	return len(saida) > 0, err
}
