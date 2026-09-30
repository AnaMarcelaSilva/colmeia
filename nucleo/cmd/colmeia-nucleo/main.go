// Núcleo da Colmeia: roda em segundo plano e é dono dos terminais. As telas se
// conectam pelo canal local (socket Unix + token), nunca por porta de rede.
package main

import (
	"context"
	"errors"
	"flag"
	"fmt"
	"log"
	"net/http"
	"os"
	"os/signal"
	"sync/atomic"
	"syscall"
	"time"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/api"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/canal"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/dados"
	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/terminal"
)

const versao = "0.1.0"

func main() {
	quantidade := flag.Int("terminais", 10, "quantidade de terminais de teste no modo demonstração")
	modoDemo := flag.Bool("demo", false, "modo demonstração: terminais de teste e cargas que escrevem comandos neles")
	flag.Parse()
	if *quantidade < 1 || *quantidade > 64 {
		log.Fatal("--terminais precisa estar entre 1 e 64")
	}
	// Fora da demonstração não há terminais de teste: os agentes de verdade
	// ganham terminal quando forem adicionados a uma tarefa.
	if !*modoDemo {
		*quantidade = 0
	}

	listener, token, fechar, err := canal.Abrir()
	if errors.Is(err, canal.ErrEmUso) {
		log.Fatal("já existe um núcleo da Colmeia rodando")
	}
	if err != nil {
		log.Fatalf("abrindo o canal local: %v", err)
	}
	defer fechar()

	dirDados, err := dados.Diretorio()
	if err != nil {
		log.Fatalf("sem diretório de dados: %v", err)
	}
	banco, err := dados.Abrir(dirDados)
	if err != nil {
		log.Fatalf("abrindo os dados: %v", err)
	}
	defer banco.Fechar()
	if err := banco.VerificarHistorico(context.Background()); err != nil {
		// Não impede o uso, mas avisa: alguém mexeu no histórico por fora.
		log.Printf("atenção: %v", err)
	}

	var bytes atomic.Int64
	pasta, _ := os.UserHomeDir()
	sessoes := make([]*terminal.Sessao, 0, *quantidade)
	for i := range *quantidade {
		prompt := fmt.Sprintf("PS1=\\[\\e[35m\\]agente-%d\\[\\e[0m\\] \\w $ ", i)
		pty, err := terminal.Iniciar([]string{"bash", "--noprofile", "--norc"}, []string{prompt}, pasta)
		if err != nil {
			log.Fatalf("abrindo terminal %d: %v", i, err)
		}
		s := terminal.NovaSessao(i, pty, &bytes)
		sessoes = append(sessoes, s)
		go s.Ler()
	}

	servidor := &api.Servidor{Sessoes: sessoes, Bytes: &bytes, Versao: versao, Demo: *modoDemo, Banco: banco, DirDados: dirDados}
	servidorHTTP := &http.Server{
		Handler:           canal.ExigirToken(token, servidor.Rotas()),
		ReadHeaderTimeout: 5 * time.Second,
		MaxHeaderBytes:    16 << 10,
	}

	parar := make(chan os.Signal, 1)
	signal.Notify(parar, syscall.SIGINT, syscall.SIGTERM)
	go func() {
		<-parar
		ctx, cancelar := context.WithTimeout(context.Background(), 2*time.Second)
		defer cancelar()
		servidorHTTP.Shutdown(ctx)
	}()

	log.Printf("núcleo %s com %d terminais no canal %s (demo: %v)", versao, *quantidade, listener.Addr(), *modoDemo)
	if err := servidorHTTP.Serve(listener); err != nil && !errors.Is(err, http.ErrServerClosed) {
		log.Printf("servidor: %v", err)
	}
	for _, s := range sessoes {
		s.Fechar()
	}
}
