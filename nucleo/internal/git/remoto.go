package git

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/url"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"time"
)

// ErrSemRemoto diz que o repositório não tem o remoto "origin".
var ErrSemRemoto = errors.New("o repositório não tem um remoto origin")

// Remoto é para onde o "origin" aponta: o servidor e, no GitHub, quem é
// dono do repositório e o nome dele.
type Remoto struct {
	Host string
	Dono string
	Repo string
}

// GitHub diz se o remoto é no GitHub (onde dá para abrir o PR pelo endereço).
func (r Remoto) GitHub() bool {
	return r.Host == "github.com" && r.Dono != "" && r.Repo != ""
}

// Caminho é "dono/repo" (vazio fora do GitHub).
func (r Remoto) Caminho() string {
	if !r.GitHub() {
		return ""
	}
	return r.Dono + "/" + r.Repo
}

// Comparar é a página do GitHub que abre um PR da branch contra a base.
func (r Remoto) Comparar(base, branch string) string {
	if !r.GitHub() || base == "" || branch == "" {
		return ""
	}
	return "https://github.com/" + r.Dono + "/" + r.Repo + "/compare/" + url.PathEscape(base) + "..." + url.PathEscape(branch) + "?expand=1"
}

// Pulls é a lista de PRs da branch no GitHub (para quem não tem o gh).
func (r Remoto) Pulls(branch string) string {
	if !r.GitHub() || branch == "" {
		return ""
	}
	return "https://github.com/" + r.Dono + "/" + r.Repo + "/pulls?q=" + url.QueryEscape("is:pr head:"+branch)
}

var nomeNoGitHub = regexp.MustCompile(`^[A-Za-z0-9._-]+$`)

// LerRemoto lê o "origin" do repositório da pasta.
func LerRemoto(ctx context.Context, dir string) (Remoto, error) {
	endereco, err := rodar(ctx, dir, "remote", "get-url", "origin")
	if err != nil || endereco == "" {
		if _, errRaiz := rodar(ctx, dir, "rev-parse", "--show-toplevel"); errRaiz != nil {
			return Remoto{}, ErrNaoRepositorio
		}
		return Remoto{}, ErrSemRemoto
	}
	return interpretarRemoto(endereco), nil
}

// interpretarRemoto entende os endereços comuns: git@github.com:dono/repo.git,
// ssh://git@github.com/dono/repo.git e https://github.com/dono/repo.git.
// O dono e o repo só valem com letras, números, ponto, traço e sublinhado.
func interpretarRemoto(endereco string) Remoto {
	endereco = strings.TrimSpace(endereco)
	var host, caminho string
	switch {
	case strings.Contains(endereco, "://"):
		u, err := url.Parse(endereco)
		if err != nil {
			return Remoto{}
		}
		host, caminho = u.Hostname(), u.Path
	case strings.Contains(endereco, ":") && !strings.Contains(strings.SplitN(endereco, ":", 2)[0], "/"):
		// A forma do scp: [usuário@]servidor:caminho.
		partes := strings.SplitN(endereco, ":", 2)
		host, caminho = partes[0], partes[1]
		if i := strings.LastIndex(host, "@"); i >= 0 {
			host = host[i+1:]
		}
	default:
		return Remoto{}
	}
	host = strings.ToLower(host)
	caminho = strings.Trim(strings.TrimSuffix(strings.TrimSuffix(caminho, "/"), ".git"), "/")
	r := Remoto{Host: host}
	partes := strings.Split(caminho, "/")
	if len(partes) == 2 && nomeNoGitHub.MatchString(partes[0]) && nomeNoGitHub.MatchString(partes[1]) {
		r.Dono, r.Repo = partes[0], partes[1]
	}
	return r
}

// Envio diz se a branch já está no origin e quantos commits ainda não
// foram: contra a cópia do origin quando ela existe, senão contra a base.
type Envio struct {
	Enviada bool
	AEnviar int
}

// LerEnvio compara a branch com o que o origin tem dela. `base` é o commit
// de onde a branch saiu (para contar os commits de uma branch nunca enviada).
func LerEnvio(ctx context.Context, dir, branch, base string) (Envio, error) {
	remota := "refs/remotes/origin/" + branch
	if _, err := rodar(ctx, dir, "rev-parse", "-q", "--verify", remota); err == nil {
		n, err := contar(ctx, dir, remota+"..HEAD")
		return Envio{Enviada: true, AEnviar: n}, err
	}
	if base == "" {
		return Envio{}, nil
	}
	n, err := contar(ctx, dir, base+"..HEAD")
	return Envio{Enviada: false, AEnviar: n}, err
}

func contar(ctx context.Context, dir, faixa string) (int, error) {
	saida, err := rodar(ctx, dir, "rev-list", "--count", faixa, "--")
	if err != nil {
		return 0, err
	}
	return strconv.Atoi(saida)
}

// Enviar manda a branch ao origin (git push -u). Usa as credenciais do git
// de quem está logado no computador, sem nunca perguntar senha: sem acesso,
// falha com a mensagem do git.
func Enviar(ctx context.Context, dir, branch string) error {
	if branch == "" || strings.HasPrefix(branch, "-") {
		return errors.New("branch inválida")
	}
	ctx, cancelar := context.WithTimeout(ctx, 90*time.Second)
	defer cancelar()
	ref := "refs/heads/" + branch
	cmd := exec.CommandContext(ctx, "git", "-C", dir, "push", "-u", "origin", ref+":"+ref)
	cmd.Env = append(os.Environ(), "GIT_TERMINAL_PROMPT=0", "GIT_EDITOR=true", "LC_ALL=C")
	if os.Getenv("GIT_SSH_COMMAND") == "" {
		cmd.Env = append(cmd.Env, "GIT_SSH_COMMAND=ssh -o BatchMode=yes")
	}
	if saida, err := cmd.CombinedOutput(); err != nil {
		texto := strings.TrimSpace(string(saida))
		if texto == "" {
			texto = err.Error()
		}
		if errors.Is(ctx.Err(), context.DeadlineExceeded) {
			texto = "o envio passou de 90 s e foi cancelado"
		}
		return fmt.Errorf("%s", texto)
	}
	return nil
}

// Checks é o resultado dos checks do PR, resumido: "ok", "falhou",
// "rodando" ou vazio sem checks.
type Checks struct {
	Resumo  string `json:"resumo"`
	Ok      int    `json:"ok"`
	Falhou  int    `json:"falhou"`
	Rodando int    `json:"rodando"`
	Total   int    `json:"total"`
}

// PR é o pull request da branch no GitHub, como o gh conta.
type PR struct {
	Numero int    `json:"numero"`
	URL    string `json:"url"`
	Titulo string `json:"titulo"`
	// aberto, rascunho, incorporado ou fechado.
	Estado string `json:"estado"`
	Checks Checks `json:"checks"`
}

// TemGh diz se o gh (a linha de comando do GitHub) está instalado.
func TemGh() bool {
	_, err := exec.LookPath("gh")
	return err == nil
}

// PullRequest pergunta ao gh pelo PR da branch. Sem PR, nil sem erro; sem
// gh, sem login ou sem rede, o erro diz o que houve.
func PullRequest(ctx context.Context, dir, branch string) (*PR, error) {
	if branch == "" || strings.HasPrefix(branch, "-") {
		return nil, nil
	}
	ctx, cancelar := context.WithTimeout(ctx, 20*time.Second)
	defer cancelar()
	cmd := exec.CommandContext(ctx, "gh", "pr", "view", branch, "--json", "number,url,title,state,isDraft,statusCheckRollup")
	cmd.Dir = dir
	cmd.Env = append(os.Environ(), "GH_PROMPT_DISABLED=1", "GH_NO_UPDATE_NOTIFIER=1", "NO_COLOR=1")
	saida, err := cmd.Output()
	if err != nil {
		var falha *exec.ExitError
		if errors.As(err, &falha) {
			texto := strings.TrimSpace(string(falha.Stderr))
			if strings.Contains(texto, "no pull requests found") {
				return nil, nil
			}
			if texto == "" {
				texto = err.Error()
			}
			return nil, fmt.Errorf("gh: %s", primeiraLinha(texto))
		}
		return nil, err
	}
	return interpretarPR(saida)
}

func primeiraLinha(texto string) string {
	if i := strings.IndexByte(texto, '\n'); i >= 0 {
		return texto[:i]
	}
	return texto
}

// interpretarPR lê o JSON do gh. Cada item do statusCheckRollup é um
// CheckRun (status e conclusion) ou um StatusContext (state).
func interpretarPR(saida []byte) (*PR, error) {
	var d struct {
		Number  int    `json:"number"`
		URL     string `json:"url"`
		Title   string `json:"title"`
		State   string `json:"state"`
		IsDraft bool   `json:"isDraft"`
		Checks  []struct {
			Tipo       string `json:"__typename"`
			Status     string `json:"status"`
			Conclusion string `json:"conclusion"`
			State      string `json:"state"`
		} `json:"statusCheckRollup"`
	}
	if err := json.Unmarshal(saida, &d); err != nil {
		return nil, fmt.Errorf("resposta do gh: %w", err)
	}
	pr := &PR{Numero: d.Number, URL: d.URL, Titulo: d.Title}
	switch {
	case d.State == "MERGED":
		pr.Estado = "incorporado"
	case d.State == "CLOSED":
		pr.Estado = "fechado"
	case d.IsDraft:
		pr.Estado = "rascunho"
	default:
		pr.Estado = "aberto"
	}
	for _, c := range d.Checks {
		pr.Checks.Total++
		switch c.Tipo {
		case "StatusContext":
			switch c.State {
			case "SUCCESS":
				pr.Checks.Ok++
			case "PENDING", "EXPECTED":
				pr.Checks.Rodando++
			default:
				pr.Checks.Falhou++
			}
		default:
			switch {
			case c.Status != "COMPLETED":
				pr.Checks.Rodando++
			case c.Conclusion == "SUCCESS", c.Conclusion == "NEUTRAL", c.Conclusion == "SKIPPED":
				pr.Checks.Ok++
			default:
				pr.Checks.Falhou++
			}
		}
	}
	switch {
	case pr.Checks.Falhou > 0:
		pr.Checks.Resumo = "falhou"
	case pr.Checks.Rodando > 0:
		pr.Checks.Resumo = "rodando"
	case pr.Checks.Ok > 0:
		pr.Checks.Resumo = "ok"
	}
	return pr, nil
}
