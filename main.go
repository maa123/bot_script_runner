package main

import (
	"net/http"

	"github.com/dop251/goja"
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
	s := new(Script)
	if err := c.Bind(s); err != nil {
		return err
	}
	result := new(Result)
	vm := goja.New()
	val, err := vm.RunString(s.Script)
	if err != nil {
		result.Error = err.Error()
		return c.JSON(http.StatusOK, result)
	}
	result.Error = ""
	result.Result = val.String()
	return c.JSON(http.StatusOK, result)
}
