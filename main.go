package main

import (
	"bufio"
	"encoding/json"
	"io"
	"log"
	"net/http"
	"os/exec"
	"runtime"
	"time"

	"github.com/labstack/echo"
)

type Script struct {
	Script string `json:"script"`
}

type Result struct {
	Result string `json:"result"`
	Error  string `json:"error"`
}

func main() {
	e := echo.New()
	e.GET("/", func(c echo.Context) error {
		return c.String(http.StatusOK, "200 OK")
	})
	e.POST("/", run)
	e.Logger.Fatal(e.Start(":7690"))
}

func run(c echo.Context) error {
	cmd_name := "./target/release/bot_script_runner"
	timeout := false
	s := new(Script)
	if err := c.Bind(s); err != nil {
		return err
	}
	input, err := json.Marshal(s)
	if err != nil {
		return err
	}

	if runtime.GOOS == "windows" {
		cmd_name = ".\\target\\release\\bot_script_runner.exe"
	}

	cmd := exec.Command(cmd_name)
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		return err
	}
	stdin, _ := cmd.StdinPipe()
	io.WriteString(stdin, string(input))
	stdin.Close()
	if err = cmd.Start(); err != nil {
		return err
	}
	ticker := *time.NewTicker(300 * time.Millisecond)
	exit := make(chan bool, 2)
	var result_str string
	go func() {
		for {
			select {
			case <-ticker.C:
				ticker.Stop()
				if cmd.ProcessState == nil || !cmd.ProcessState.Exited() {
					if err := cmd.Process.Kill(); err != nil {
						log.Print("Stop Error")
					}
					timeout = true
					exit <- true
				}
				return
			}
		}
	}()
	go func() {
		defer func() { exit <- false }()
		scanner := bufio.NewScanner(stdout)
		for scanner.Scan() {
			result_str += scanner.Text()
		}
		if err := cmd.Wait(); err != nil {
			log.Print("cmd error")
		}
	}()
	isKill := <-exit
	result := new(Result)
	result.Result = ""
	result.Error = ""
	if isKill {
		result.Error = "Error"
		if timeout {
			result.Error = "Timeout"
		}
	} else {
		if err := json.Unmarshal([]byte(result_str), &result); err != nil {
			return err
		}
	}

	return c.JSON(http.StatusOK, result)
}
