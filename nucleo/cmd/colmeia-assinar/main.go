// colmeia-assinar monta e assina o manifesto de uma release (colmeia.json e
// colmeia.json.sig), que a Colmeia confere antes de se atualizar.
//
//	colmeia-assinar gerar <arquivo>    cria um par de chaves: a privada no arquivo (0600), a pública na saída
//	colmeia-assinar manifesto -versao 0.5.0 -changelog CHANGELOG.md -saida dist pacote...
//
// A chave privada vem da variável COLMEIA_ASSINATURA (base64), nunca de argumento.
package main

import (
	"crypto/ed25519"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"io"
	"log"
	"os"
	"path/filepath"
	"strings"

	"github.com/AnaMarcelaSilva/colmeia/nucleo/internal/atualizacao"
)

func main() {
	log.SetFlags(0)
	if len(os.Args) < 2 {
		log.Fatal("uso: colmeia-assinar gerar <arquivo> | manifesto -versao X -changelog CHANGELOG.md -saida pasta pacote...")
	}
	var err error
	switch os.Args[1] {
	case "gerar":
		err = gerar(os.Args[2:])
	case "manifesto":
		err = manifesto(os.Args[2:])
	default:
		err = fmt.Errorf("comando desconhecido: %s", os.Args[1])
	}
	if err != nil {
		log.Fatal(err)
	}
}

func gerar(args []string) error {
	if len(args) != 1 {
		return errors.New("uso: colmeia-assinar gerar <arquivo>")
	}
	pub, priv, err := ed25519.GenerateKey(rand.Reader)
	if err != nil {
		return err
	}
	arquivo, err := os.OpenFile(args[0], os.O_CREATE|os.O_EXCL|os.O_WRONLY, 0o600)
	if err != nil {
		return err
	}
	if _, err := fmt.Fprintln(arquivo, base64.StdEncoding.EncodeToString(priv.Seed())); err != nil {
		arquivo.Close()
		return err
	}
	if err := arquivo.Close(); err != nil {
		return err
	}
	fmt.Println(base64.StdEncoding.EncodeToString(pub))
	return nil
}

func manifesto(args []string) error {
	opcoes := flag.NewFlagSet("manifesto", flag.ContinueOnError)
	versao := opcoes.String("versao", "", "versão da release, sem o v")
	changelog := opcoes.String("changelog", "", "CHANGELOG.md de onde saem as notas")
	saida := opcoes.String("saida", ".", "pasta onde gravar colmeia.json e colmeia.json.sig")
	if err := opcoes.Parse(args); err != nil {
		return err
	}
	*versao = strings.TrimPrefix(*versao, "v")
	semente, err := base64.StdEncoding.DecodeString(strings.TrimSpace(os.Getenv("COLMEIA_ASSINATURA")))
	if err != nil || len(semente) != ed25519.SeedSize {
		return errors.New("COLMEIA_ASSINATURA precisa ter a chave privada (base64)")
	}
	priv := ed25519.NewKeyFromSeed(semente)
	texto, err := os.ReadFile(*changelog)
	if err != nil {
		return err
	}
	notas, achou := atualizacao.NotasDoChangelog(string(texto), *versao)
	if !achou {
		return fmt.Errorf("o CHANGELOG não tem a seção ## [%s]", *versao)
	}
	m := atualizacao.Manifesto{Versao: *versao, Notas: notas, Arquivos: map[string]atualizacao.Arquivo{}}
	for _, caminho := range opcoes.Args() {
		a, err := resumir(caminho)
		if err != nil {
			return err
		}
		m.Arquivos[filepath.Base(caminho)] = a
	}
	bruto, err := json.MarshalIndent(m, "", "  ")
	if err != nil {
		return err
	}
	assinatura := atualizacao.Assinar(priv, bruto)
	// Confere com a chave que a Colmeia traz: uma chave errada para aqui, não na casa de quem atualiza.
	if _, err := atualizacao.Verificar(atualizacao.ChavePublica, bruto, assinatura); err != nil {
		return fmt.Errorf("a assinatura não confere com a chave pública da Colmeia: %w", err)
	}
	if err := os.WriteFile(filepath.Join(*saida, atualizacao.NomeManifesto), bruto, 0o644); err != nil {
		return err
	}
	return os.WriteFile(filepath.Join(*saida, atualizacao.NomeAssinatura), []byte(assinatura+"\n"), 0o644)
}

func resumir(caminho string) (atualizacao.Arquivo, error) {
	arquivo, err := os.Open(caminho)
	if err != nil {
		return atualizacao.Arquivo{}, err
	}
	defer arquivo.Close()
	hash := sha256.New()
	n, err := io.Copy(hash, arquivo)
	if err != nil {
		return atualizacao.Arquivo{}, err
	}
	return atualizacao.Arquivo{SHA256: hex.EncodeToString(hash.Sum(nil)), Tamanho: n}, nil
}
