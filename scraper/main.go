package main

import (
	"log"
	"regexp"
	"time"

	"github.com/gocolly/colly/v2"
)

const (
	here          = `https://www.fia.com/documents/championships/fia-formula-one-world-championship-14/season/season-2022-2005/event/Belgian%20Grand%20Prix/`
	linksSelector = "li.document-row a"
	offenceRe     = `(Offence|Decision) - Car [0-9]{1,2} - (PU elements|RNC Changes)`
	newElemsRe    = `New (PU elements|RNCs)`
	ua            = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36"
)

func main() {
	oRe := regexp.MustCompile(offenceRe)
	nERe := regexp.MustCompile(newElemsRe)
	c := colly.NewCollector(colly.UserAgent(ua), colly.AllowedDomains("www.fia.com"))
	err := c.Limit(&colly.LimitRule{
		Delay:       10 * time.Second,
		DomainRegexp: `fia\.com`,
		Parallelism: 1,
	})
	if err != nil {
		log.Fatal(err)
	}

	c.OnHTML(linksSelector, func(h *colly.HTMLElement) {
		link := h.Attr("href")
		if oRe.MatchString(link) {
			log.Printf("downloading offence file %s", link)
		} else if nERe.MatchString(link) {
			log.Printf("downloading new elements file %s", link)
		}
	})
	err = c.Visit(here)
	if err != nil {
		log.Fatal(err)
	}
}
