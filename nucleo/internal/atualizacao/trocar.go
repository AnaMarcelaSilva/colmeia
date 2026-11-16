package atualizacao

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
)

// Quanto a troca espera o núcleo antigo sair (os agentes têm uns segundos
// para salvar) e o novo responder.
const (
	esperaSaida    = 60 * time.Second
	esperaResposta = 20 * time.Second
)

// Trocar é o programa à parte que faz a troca (colmeia-nucleo --trocar):
// espera o núcleo de pid sair, põe os programas novos no lugar, abre o
// núcleo novo e confere se ele responde com a versão nova. Se não
// responder, devolve os programas e o banco de antes e abre a versão
// anterior. Em qualquer caso, abre a tela de novo se ela estava aberta.
func Trocar(dir string, pid int, abrirTela bool) error {
	if registro, err := os.OpenFile(filepath.Join(dir, "registro.log"), os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0o600); err == nil {
		defer registro.Close()
		log.SetOutput(registro)
	}
	bruto, err := os.ReadFile(filepath.Join(dir, "pendente.json"))
	if err != nil {
		return fmt.Errorf("sem troca pendente: %w", err)
	}
	var p pendente
	if err := json.Unmarshal(bruto, &p); err != nil || p.DirBin == "" || p.DirDados == "" {
		return errors.New("troca pendente fora do formato")
	}
	log.Printf("trocando %s por %s em %s", p.De, p.Para, p.DirBin)
	if !esperarSair(pid, esperaSaida) {
		log.Printf("o núcleo %d não saiu; nada trocado", pid)
		os.Remove(filepath.Join(dir, "pendente.json"))
		return errors.New("o núcleo antigo não saiu")
	}

	nova, anterior := filepath.Join(dir, "nova"), filepath.Join(dir, "anterior")
	err = trocarProgramas(nova, p.DirBin)
	if err == nil {
		err = abrirNucleo(p.DirBin, p.Para)
	}
	r := resultado{OK: err == nil, De: p.De, Para: p.Para}
	if err != nil {
		log.Printf("a versão %s falhou (%v); voltando para a %s", p.Para, err, p.De)
		r.Erro = err.Error()
		gravar(filepath.Join(dir, "falhou"), []byte(p.Para+"\n"), 0o600)
		if err := trocarProgramas(anterior, p.DirBin); err != nil {
			log.Printf("devolvendo os programas: %v", err)
		}
		if err := devolverBanco(filepath.Join(anterior, "colmeia.db"), p.DirDados); err != nil {
			log.Printf("devolvendo o banco: %v", err)
		}
		if err := abrirNucleo(p.DirBin, p.De); err != nil {
			log.Printf("a versão anterior também não abriu: %v", err)
		}
	} else {
		log.Printf("versão %s aberta", p.Para)
	}
	bruto, _ = json.Marshal(r)
	gravar(filepath.Join(dir, "resultado.json"), bruto, 0o600)
	os.Remove(filepath.Join(dir, "pendente.json"))
	os.RemoveAll(nova)
	if abrirTela {
		if err := iniciarSolto(filepath.Join(p.DirBin, "colmeia")); err != nil {
			log.Printf("abrindo a tela: %v", err)
		}
	}
	return nil
}

func esperarSair(pid int, prazo time.Duration) bool {
	limite := time.Now().Add(prazo)
	for time.Now().Before(limite) {
		if !processoVivo(pid) {
			return true
		}
		time.Sleep(200 * time.Millisecond)
	}
	return false
}

// trocarProgramas copia cada programa de origem para um temporário ao lado
// do destino e troca o nome: quem estiver rodando o antigo segue com ele.
func trocarProgramas(origem, dirBin string) error {
	for _, p := range programas {
		de := filepath.Join(origem, p.nome)
		if _, err := os.Stat(de); err != nil {
			if p.obrigatorio {
				return fmt.Errorf("falta %s: %w", p.nome, err)
			}
			continue
		}
		para := filepath.Join(dirBin, p.nome)
		tmp := filepath.Join(dirBin, "."+p.nome+".novo")
		if err := copiar(de, tmp, 0o755); err != nil {
			os.Remove(tmp)
			return err
		}
		if err := os.Rename(tmp, para); err != nil {
			os.Remove(tmp)
			return err
		}
	}
	return nil
}

// devolverBanco põe a cópia de antes no lugar (o núcleo novo pode ter
// mudado o esquema ao abrir).
func devolverBanco(copia, dirDados string) error {
	if _, err := os.Stat(copia); err != nil {
		return err
	}
	destino := filepath.Join(dirDados, "colmeia.db")
	tmp := destino + ".voltando"
	if err := copiar(copia, tmp, 0o600); err != nil {
		os.Remove(tmp)
		return err
	}
	os.Remove(destino + "-wal")
	os.Remove(destino + "-shm")
	return os.Rename(tmp, destino)
}

// abrirNucleo abre o núcleo de dirBin e espera ele responder com a versão.
// Sem resposta, encerra o que abriu.
func abrirNucleo(dirBin, versao string) error {
	comando := exec.Command(filepath.Join(dirBin, "colmeia-nucleo"))
	soltar(comando)
	if err := comando.Start(); err != nil {
		return err
	}
	saiu := make(chan struct{})
	go func() {
		comando.Wait()
		close(saiu)
	}()
	limite := time.Now().Add(esperaResposta)
	for time.Now().Before(limite) {
		select {
		case <-saiu:
			return errors.New("o núcleo novo fechou ao abrir")
		case <-time.After(250 * time.Millisecond):
		}
		// Outra versão respondendo é um núcleo que não é este: segue esperando.
		if v, err := versaoNoCanal(); err == nil && v == versao {
			return nil
		}
	}
	comando.Process.Kill()
	<-saiu
	return errors.New("o núcleo novo não respondeu")
}

// versaoNoCanal pergunta a versão ao núcleo que está no canal.
func versaoNoCanal() (string, error) {
	dir, err := canal.Diretorio()
	if err != nil {
		return "", err
	}
	token, err := os.ReadFile(filepath.Join(dir, canal.NomeToken))
	if err != nil {
		return "", err
	}
	cliente := &http.Client{Timeout: 2 * time.Second, Transport: &http.Transport{
		DialContext: func(ctx context.Context, _, _ string) (net.Conn, error) {
			return (&net.Dialer{}).DialContext(ctx, "unix", filepath.Join(dir, canal.NomeSocket))
		},
	}}
	pedido, _ := http.NewRequest(http.MethodGet, "http://colmeia/v1/versao", nil)
	pedido.Header.Set("Authorization", "Bearer "+strings.TrimSpace(string(token)))
	resposta, err := cliente.Do(pedido)
	if err != nil {
		return "", err
	}
	defer resposta.Body.Close()
	var v struct {
		Nucleo string `json:"nucleo"`
	}
	if err := json.NewDecoder(resposta.Body).Decode(&v); err != nil {
		return "", err
	}
	return v.Nucleo, nil
}
