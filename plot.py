import os
import json
import matplotlib.pyplot as plt
from collections import defaultdict
from datetime import datetime

plt.style.use('ggplot')

def make_plots(output_dir):
    os.mkdir(os.path.join(output_dir, 'plots'))
    output = json.load(open(os.path.join(output_dir, 'output.json')))
    history = output['history']
    stats = defaultdict(list)

    # config = json.load(open(os.path.join(output_dir, 'config.json')))
    # config.pop('map')
    config = {}

    # Get neighborhood-specific stats
    by_neighb = [h.pop('neighborhoods') for h in history]
    neighborhoods = defaultdict(lambda: defaultdict(list))
    for h in by_neighb:
        for neighb, sts in h.items():
            for k, v in sts.items():
                neighborhoods[k][neighb].append(v)

    # Get landlord-specific stats
    by_landlord = [h.pop('landlords') for h in history]
    landlords = defaultdict(lambda: defaultdict(list))
    for h in by_landlord:
        for landlord, sts in h.items():
            for k, v in sts.items():
                landlords[k][landlord].append(v)

    for month in history:
        for k, v in month.items():
            stats[k].append(v)

    fnames = []

    # Rents
    plt.title('rents')
    for k in ['mean_rent_per_area', 'mean_adjusted_rent_per_area']:
        vals = stats[k]
        plt.plot(range(len(vals)), vals, label=k)
    plt.legend()
    fnames.append('rents.png')
    plt.savefig(os.path.join(output_dir, 'plots/rents.png'))
    plt.close()

    # DOMA fund
    plt.title('doma_fund')
    for k in ['doma_property_fund', 'mean_value', 'min_value']:
        vals = stats[k]
        plt.plot(range(len(vals)), stats[k], label=k)
    plt.legend()
    fnames.append('doma_fund.png')
    plt.savefig(os.path.join(output_dir, 'plots/doma_fund.png'))
    plt.close()
    del stats['min_value']


    for k, vals in stats.items():
        solo = True

        # Show per neighborhood, if available
        if k in neighborhoods:
            solo = False
            plt.title(k)
            plt.plot(range(len(vals)), vals, label='All')
            for neighb, vs in neighborhoods[k].items():
                plt.plot(range(len(vals)), vs, label='Neighborhood {}'.format(neighb))
            plt.legend()
            fnames.append('{}_neighb.png'.format(k))
            plt.savefig(os.path.join(output_dir, 'plots/{}_neighb.png'.format(k)))
            plt.close()

        if k in landlords:
            solo = False
            plt.title(k)
            plt.plot(range(len(vals)), vals, label='All')
            for id, vs in landlords[k].items():
                if id == "-1":
                    plt.plot(range(len(vals)), vs, label='DOMA', color='#f771b4')
                else:
                    plt.plot(range(len(vals)), vs, label='Landlord {}'.format(id))
            plt.legend()
            fnames.append('{}_landlords.png'.format(k))
            plt.savefig(os.path.join(output_dir, 'plots/{}_landlords.png'.format(k)))
            plt.close()

        if solo:
            plt.title(k)
            plt.plot(range(len(vals)), vals)
            fnames.append('{}.png'.format(k))
            plt.savefig(os.path.join(output_dir, 'plots/{}.png'.format(k)))
            plt.close()

    with open(os.path.join(output_dir, 'plots/index.html'), 'w') as f:
        html = '''
            <html>
            <body style="font-family:monospace;">
                <h3>Generated on {dt}</h3>
                <div>
                    {meta}
                    {config}
                </div>
                <div>
                    {imgs}
                </div>
            </body>
            </html>
        '''.format(
            dt=datetime.now().isoformat(),
            config=json.dumps(config),
            meta=', '.join('{}: {}'.format(k, v) for k, v in output['meta'].items()),
            imgs='\n'.join(['<img style="width:400px;" src="{}">'.format(fname) for fname in fnames]))
        f.write(html)


if __name__ == '__main__':
    make_plots('.')
