package main

import (
	"log"
	"regexp"
	"strings"

	"code.sajari.com/docconv"
)

const (
	penaltyRe      = `Decision\n\n(?P<Penalty>[\s\S]*?){1}\n\n`
	replacementsRe = `(Number\n\nCar\n\nDriver\n\n(?P<DriverNumber>(?:[0-9][0-9]\n)+)[\s\S]*?Previously used\s*[\s\S]*?(?P<EnginePart>[^\n]+))+`
	numberReg      = `Car (?P<Number>[0-9][0-9])`
)

func main() {
	/* newPUsFilename := `../2023 Saudi Arabian Grand Prix - New PUs for this Event.pdf`
	newRNCsFilename := `../2022 Belgian Grand Prix - New RNCs for this Event.pdf` */
	offenceFilename := `../2022 Belgian Grand Prix - Offence - Car 22 - PU elements.pdf`
	number := regexp.MustCompile(numberReg).FindStringSubmatch(offenceFilename)[1]

	filename := offenceFilename
	rgx := penaltyRe

	// to convert from HTTP response:
	// docconv.Convert(resp.Body, "application/pdf", true)

	res, err := docconv.ConvertPath(filename)
	if err != nil {
		log.Fatal(err)
	}
	re, err := regexp.Compile(rgx)
	if err != nil {
		log.Fatal(err)
	}
	if filename != offenceFilename {
		driversNParts := re.FindAllStringSubmatch(res.Body, -1)
		for _, s := range driversNParts {
			drivers := strings.Split(s[2], "\n")
			drivers = drivers[:len(drivers)-1]
			log.Printf("driver(s) %s requested a new %s", strings.Join(drivers, ", "), s[3])
		}
	} else {
		log.Printf("driver %s has been given the following penalty: %s", number, re.FindStringSubmatch(res.Body)[1])
	}
}
