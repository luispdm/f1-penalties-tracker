package main

import (
	"log"
	"net/url"
	"regexp"
	"time"

	"github.com/gocolly/colly/v2"
)

const (
	here          = `https://www.fia.com/documents/championships/fia-formula-one-world-championship-14/season/season-2022-2005/event/Belgian%20Grand%20Prix/`
	linksSelector = "li.document-row a"
	offenceRe     = `(Offence|Decision) - Car [0-9]{1,2} - (PU elements|RNC Changes)`
	newElemsRe    = `New (PU elements|RNCs)`
	upToNowRe     = `(PU elements|RNCs) used per driver up to now`
	ua            = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/109.0.0.0 Safari/537.36"
	// up to now needed in order to be consistent: when offences occur after up to now is published,
	// parts counter can't be increased because natural language is not processed
	
	// RNC issues | allocations : Friday's bonus allocations - up to 4 (they don't count toward the limit).
	// Retrieve the note from the pdf and store it as a comment
)

func main() {
	oRe := regexp.MustCompile(offenceRe)
	nERe := regexp.MustCompile(newElemsRe)
	uTNRe := regexp.MustCompile(upToNowRe)
	c := colly.NewCollector(colly.UserAgent(ua), colly.AllowedDomains("www.fia.com"))
	err := c.Limit(&colly.LimitRule{
		Delay:        10 * time.Second,
		DomainRegexp: `fia\.com`,
		Parallelism:  1,
	})
	if err != nil {
		log.Fatal(err)
	}

	c.OnHTML(linksSelector, func(h *colly.HTMLElement) {
		link := h.Attr("href")
		if oRe.MatchString(link) {
			parsed, err := url.Parse(link)
			if err != nil {
				log.Fatal(err)
			}
			log.Printf("found offence file %s", parsed)
		} else if nERe.MatchString(link) {
			parsed, err := url.Parse(link)
			if err != nil {
				log.Fatal(err)
			}
			log.Printf("found new elements file %s", parsed)
		} else if uTNRe.MatchString(link) {
			parsed, err := url.Parse(link)
			if err != nil {
				log.Fatal(err)
			}
			log.Printf("found up to now file %s", parsed)
		}
	})
	err = c.Visit(here)
	if err != nil {
		log.Fatal(err)
	}
}
